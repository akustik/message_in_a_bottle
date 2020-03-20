extern crate redis;

use redis::Commands;

use std::env;
use std::error;
use std::fmt;
use std::time::Duration;
use std::sync::mpsc::{self, TryRecvError};

use serde::{Serialize, Deserialize};

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::message::NotificationChannel;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug, Clone)]
struct StorageError;

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "invalid first item to double")
    }
}

impl error::Error for StorageError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        None
    }
}

#[derive(Serialize, Deserialize, Debug, Hash)]
pub struct BottleMessage {
    msg: String
}

pub trait Storage {
    fn health(&self) -> Result<()>;
    fn store(&self, msg: BottleMessage) -> Result<BottleMessage>;
    fn subscribe(&self, term: mpsc::Receiver<String>, notification_channel: &dyn NotificationChannel) -> Result<()>;
}

pub struct RedisStorage {

}

impl Storage for RedisStorage {
    fn health(&self) -> Result<()> {
        let result: redis::RedisResult<()> = execute_redis_command(|con: &mut redis::Connection| {
            redis::cmd("SETEX").arg("health").arg(1).arg(42).query(con)
        });

        to_result(result)
    }

    fn store(&self, bottle: BottleMessage) -> Result<BottleMessage> {
        let hash = calculate_hash(&bottle);
        let expire_hash = format!("trigger:{}", hash);
        let trigger_expiration_in_seconds: usize = 1;
        let key_expiration_in_seconds: usize = 60;
        let result: redis::RedisResult<()> = execute_redis_command(|con: &mut redis::Connection| {
            redis::pipe().atomic()
            .set_ex(expire_hash, "", trigger_expiration_in_seconds)
            .set_ex(hash, &bottle.msg, key_expiration_in_seconds)
            .query(con)
        });

        to_result(result.map(|_| bottle))
    }

    fn subscribe(&self, term: mpsc::Receiver<String>, notification_channel: &dyn NotificationChannel) -> Result<()> {
        let result = execute_redis_command(|con: &mut redis::Connection| {
            let config_parameter = "notify-keyspace-events";
            let config_value = "Exg";
            
            let config = redis::cmd("CONFIG")
            .arg("SET")
            .arg(config_parameter)
            .arg(config_value)
            .query(con);

            let mut pubsub = con.as_pubsub();

            let subscription = config.and_then(|_: ()| {
                pubsub.psubscribe("__keyevent@0__:expired")
            }).and_then(|_| {
                pubsub.set_read_timeout(Some(Duration::from_millis(5000)))
            });

            match subscription {
                Ok(_) => {
                    println!("Listening for notifications...");
            
                    loop {
                        let msg = pubsub.get_message();
            
                        match msg {
                            Ok(m) => {
                                let payload : String = m.get_payload()?;
                                println!("channel '{}': payload '{}'", m.get_channel_name(), payload);
            
                                if payload.starts_with("trigger:") {
                                    let key = payload.replace("trigger:", "");
                                    let msg: String = execute_redis_command(|con2: &mut redis::Connection| con2.get(key))?;
                                    notification_channel.notify(msg);
                                }
                            }
                            Err(e) => println!("No notifications, {}", e)
                        }
            
                        match term.try_recv() {
                            Ok(_) | Err(TryRecvError::Disconnected) => {
                                break;
                            }
                            Err(TryRecvError::Empty) => {}
                        }
                    }
        
                    Ok(())
                },
                Err(_) => subscription
            }
        });

        to_result(result)
    }
}

fn to_result<T>(result: redis::RedisResult<T>) -> Result<T> {
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            println!("Redis operation failed '{}'", e);
            Err(Box::new(StorageError))
        }
    }
}

fn execute_redis_command<T, C: FnOnce(&mut redis::Connection) -> redis::RedisResult<T>>(command: C) -> redis::RedisResult<T> {
    let url = env::var("REDISCLOUD_URL").expect("$REDISCLOUD_URL");
    let client = redis::Client::open(url)?;
    let mut con = client.get_connection()?;
    command(&mut con)
}

fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}