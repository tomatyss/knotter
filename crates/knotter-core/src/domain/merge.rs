#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MergeCandidateReason {
    EmailDuplicate,
    EmailNameAmbiguous,
    VcfAmbiguousEmail,
    VcfAmbiguousPhoneName,
    TelegramUsernameAmbiguous,
    TelegramHandleAmbiguous,
    TelegramNameAmbiguous,
}

impl MergeCandidateReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            MergeCandidateReason::EmailDuplicate => "email-duplicate",
            MergeCandidateReason::EmailNameAmbiguous => "email-name-ambiguous",
            MergeCandidateReason::VcfAmbiguousEmail => "vcf-ambiguous-email",
            MergeCandidateReason::VcfAmbiguousPhoneName => "vcf-ambiguous-phone-name",
            MergeCandidateReason::TelegramUsernameAmbiguous => "telegram-username-ambiguous",
            MergeCandidateReason::TelegramHandleAmbiguous => "telegram-handle-ambiguous",
            MergeCandidateReason::TelegramNameAmbiguous => "telegram-name-ambiguous",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "email-duplicate" => Some(MergeCandidateReason::EmailDuplicate),
            "email-name-ambiguous" => Some(MergeCandidateReason::EmailNameAmbiguous),
            "vcf-ambiguous-email" => Some(MergeCandidateReason::VcfAmbiguousEmail),
            "vcf-ambiguous-phone-name" => Some(MergeCandidateReason::VcfAmbiguousPhoneName),
            "telegram-username-ambiguous" => Some(MergeCandidateReason::TelegramUsernameAmbiguous),
            "telegram-handle-ambiguous" => Some(MergeCandidateReason::TelegramHandleAmbiguous),
            "telegram-name-ambiguous" => Some(MergeCandidateReason::TelegramNameAmbiguous),
            _ => None,
        }
    }

    pub const fn is_auto_merge_safe(self) -> bool {
        matches!(
            self,
            MergeCandidateReason::EmailDuplicate | MergeCandidateReason::VcfAmbiguousPhoneName
        )
    }

    pub const fn all() -> &'static [MergeCandidateReason] {
        &[
            MergeCandidateReason::EmailDuplicate,
            MergeCandidateReason::EmailNameAmbiguous,
            MergeCandidateReason::VcfAmbiguousEmail,
            MergeCandidateReason::VcfAmbiguousPhoneName,
            MergeCandidateReason::TelegramUsernameAmbiguous,
            MergeCandidateReason::TelegramHandleAmbiguous,
            MergeCandidateReason::TelegramNameAmbiguous,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::MergeCandidateReason;

    #[test]
    fn parse_round_trip() {
        for reason in MergeCandidateReason::all() {
            let value = reason.as_str();
            let parsed = MergeCandidateReason::parse(value).expect("parse reason");
            assert_eq!(*reason, parsed);
        }
    }

    #[test]
    fn parse_unknown_returns_none() {
        assert!(MergeCandidateReason::parse("unknown").is_none());
    }
}
