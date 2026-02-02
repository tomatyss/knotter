pub mod contact;
pub mod contact_date;
pub mod email;
pub mod ids;
pub mod interaction;
pub mod merge;
pub mod phone;
pub mod tag;

pub use contact::Contact;
pub use contact_date::{normalize_contact_date_label, ContactDate, ContactDateKind};
pub use email::normalize_email;
pub use ids::{ContactDateId, ContactId, InteractionId, MergeCandidateId, TagId};
pub use interaction::{Interaction, InteractionKind};
pub use merge::MergeCandidateReason;
pub use phone::normalize_phone_for_match;
pub use tag::{normalize_tag_name, Tag, TagName};
