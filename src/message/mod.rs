pub mod sendgrid;
pub mod mailgun;

pub trait NotificationChannel {
    fn request_confirmation(&self, dest: String, confirmation_link: String);
    fn notify(&self, dest: String, msg: String);
}

pub use sendgrid::SendGrid as SendGrid;
pub use mailgun::Mailgun as Mailgun;

pub fn default_channel() -> &'static dyn NotificationChannel {
    &Mailgun{}
}

use reqwest::blocking::ClientBuilder as BlockingClientBuilder;

