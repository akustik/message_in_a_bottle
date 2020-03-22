use crate::message::NotificationChannel;
use crate::message::BlockingClientBuilder;
use crate::util::env_or_fail;

use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
struct MailgunMessageDynamicTemplateData {
    msg: String
}

#[derive(Serialize, Deserialize, Debug)]
struct MailgunConfirmationDynamicTemplateData {
    confirmation_link: String
}

pub struct Mailgun {

}

impl NotificationChannel for Mailgun {
    fn request_confirmation(&self, dest: String, confirmation_link: String) {
        send_email(
            dest, 
            "Hello from message in a bottle!".to_string(),
            "msg-in-a-bottle-confirmation".to_string(), 
            serde_json::to_string(&MailgunConfirmationDynamicTemplateData{confirmation_link: confirmation_link.clone()}).unwrap()
        );
    }

    fn notify(&self, dest: String, msg: String) {
        send_email(
            dest, 
            "🍾 You got a message in a bottle!".to_string(),
            "msg-in-a-bottle-v1".to_string(), 
            serde_json::to_string(&MailgunMessageDynamicTemplateData{msg: msg.clone()}).unwrap()
        );
    }
}

fn send_email(to: String, subject: String, template: String, variables_json: String) {
    let domain = env_or_fail("MAILGUN_DOMAIN");
    let from = env_or_fail("MAILGUN_SMTP_LOGIN");
    let api_key = env_or_fail("MAILGUN_API_KEY");

    let url = format!("https://api.mailgun.net/v3/{}/messages", domain);
 
    let form = reqwest::blocking::multipart::Form::new()
        .text("from", from)
        .text("to", to)
        .text("subject", subject)
        .text("template", template)
        .text("h:X-Mailgun-Variables", variables_json)
    ;

    let client = BlockingClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .build().expect("Unable to create sync client");
    
    let response = client
        .post(&url)
        .basic_auth("api", Some(api_key))
        .multipart(form)
        .send();

    match response {
        Ok(r) => println!("Message sent, status: {}, response: {}", r.status(), r.text().unwrap()),
        Err(e) => println!("Unable to send message: {}", e)
    } 
}