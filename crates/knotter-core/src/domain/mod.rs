pub mod contact;
pub mod email;
pub mod ids;
pub mod interaction;
pub mod tag;

pub use contact::Contact;
pub use email::normalize_email;
pub use ids::{ContactId, InteractionId, MergeCandidateId, TagId};
pub use interaction::{Interaction, InteractionKind};
pub use tag::{normalize_tag_name, Tag, TagName};
