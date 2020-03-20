use reqwest::blocking::ClientBuilder as BlockingClientBuilder;
use serde::{Serialize, Deserialize};
use std::env;

#[derive(Serialize, Deserialize, Debug)]
struct SendGridMessage {
    personalizations: [SendGridMessagePersonalizations; 1],
    from: SendGridMessageAddress,
    reply_to: SendGridMessageAddress,
    template_id: String
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SendGridMessageAddress {
    email: String,
    name: String
}

#[derive(Serialize, Deserialize, Debug)]
struct SendGridMessagePersonalizations {
    to: [SendGridMessageAddress; 1],
    dynamic_template_data: SendGridMessageDynamicTemplateData
}

#[derive(Serialize, Deserialize, Debug)]
struct SendGridMessageDynamicTemplateData {
    msg: String
}

pub trait NotificationChannel {
    fn notify(&self, msg: String);
}

pub struct SendGrid {

}

impl NotificationChannel for SendGrid {
    fn notify(&self, msg: String) {
        let api_key = env::var("SENDGRID_API_KEY").expect("$SENDGRID_API_KEY");
    
        let msg = SendGridMessage {
            from: SendGridMessageAddress {
                email: "towalkaway@gmail.com".to_string(),
                name: "Message in a Bottle".to_string()
            },
            reply_to: SendGridMessageAddress {
                email: "towalkaway@gmail.com".to_string(),
                name: "Message in a Bottle".to_string()
            },
            template_id: "d-3e85b81589ac4a76947baa9b13e2dc05".to_string(),
            personalizations: [ SendGridMessagePersonalizations {
                to: [SendGridMessageAddress {
                    email: "towalkaway@gmail.com".to_string(),
                    name: "Message in a Bottle".to_string()
                }],
                dynamic_template_data: SendGridMessageDynamicTemplateData {
                    msg: msg
                }
            }]
        };
    
        let json_content = serde_json::to_string_pretty(&msg).unwrap();
        println!("Going to send an email: {}", json_content);
    
        let client = BlockingClientBuilder::new()
            .danger_accept_invalid_certs(true)
            .build().expect("Unable to create sync client");
        
        let response = client
            .post("https://api.sendgrid.com/v3/mail/send")
            .bearer_auth(api_key)
            .json(&msg)
            .send();
        
        match response {
            Ok(r) => println!("Message sent, status: {}, response: {}", r.status(), r.text().unwrap()),
            Err(e) => println!("Unable to send message: {}", e)
        }   
    }
}


