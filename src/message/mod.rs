pub mod sendgrid;

pub trait NotificationChannel {
    fn notify(&self, msg: String);
}

pub use sendgrid::SendGrid as SendGrid;


