pub mod sendgrid;
pub mod mailgun;

pub trait NotificationChannel {
    fn notify(&self, msg: String);
}

pub use sendgrid::SendGrid as SendGrid;
pub use mailgun::Mailgun as Mailgun;

use reqwest::blocking::ClientBuilder as BlockingClientBuilder;

