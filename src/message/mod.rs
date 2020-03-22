pub mod sendgrid;
pub mod mailgun;

pub trait NotificationChannel {
    fn request_confirmation(&self, dest: String, confirmation_link: String);
    fn notify(&self, dest: String, msg: String);
}

pub use sendgrid::SendGrid as SendGrid;
pub use mailgun::Mailgun as Mailgun;

pub use Mailgun as DefaultChannel;

use reqwest::blocking::ClientBuilder as BlockingClientBuilder;

