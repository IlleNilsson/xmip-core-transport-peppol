//! The settings a Peppol Location takes, declared once and read through
//! (ADR-0064, amendment 2026-09-26).

use transport::Configured;
use transport::error::Result;
use xcore::settings::{Applies, Fixed, Kind, Presence, Read, Setting, Settings};

use crate::{BILLING_INVOICE, BILLING_PROCESS, Identifier, Participant, PeppolTransport};

impl Configured for PeppolTransport {
    /// The address is the access point: the partner's a Send Location posts
    /// to, `https://ap.example/as4` or `as4://host:port/as4`, or the one a
    /// Receive Location listens at.
    const SETTINGS: &'static Settings = &Settings {
        technology: env!("CARGO_PKG_NAME"),
        settings: &[
            Setting {
                name: "participant",
                kind: Kind::Text,
                presence: Presence::Required,
                meaning: "The participant a Location speaks as, `0088:…` or qualified by its \
                          scheme.",
                applies: Applies::Both,
            },
            Setting {
                name: "partner",
                kind: Kind::Text,
                presence: Presence::Required,
                meaning: "The participant a Send Location sends to when its target names none.",
                applies: Applies::Send,
            },
            Setting {
                name: "document",
                kind: Kind::Text,
                presence: Presence::Default(Fixed::Text(BILLING_INVOICE)),
                meaning: "The document type carried, under the Peppol document scheme.",
                applies: Applies::Both,
            },
            Setting {
                name: "process",
                kind: Kind::Text,
                presence: Presence::Default(Fixed::Text(BILLING_PROCESS)),
                meaning: "The process the document belongs to, under the Peppol process scheme.",
                applies: Applies::Both,
            },
            Setting {
                name: "timeout",
                kind: Kind::Duration,
                presence: Presence::Optional,
                meaning: "How long an access point that stops mid-message is waited on; \
                          unbounded when left out.",
                applies: Applies::Both,
            },
        ],
    };

    /// The signing certificate comes through the Location's credentials,
    /// never a setting; a Receive Location sends to no partner, so its own
    /// participant stands in for one.
    fn configured(address: &str, settings: &Read) -> Result<Self> {
        let me = Participant::parse(settings.text("participant"))?;
        let partner = match settings.optional_text("partner") {
            Some(partner) => Participant::parse(partner)?,
            None => me.clone(),
        };
        let transport = Self::new(address, me, partner).carrying(
            Identifier::document(settings.text("document")),
            Identifier::process(settings.text("process")),
        );
        Ok(match settings.optional_duration("timeout") {
            Some(timeout) => transport.timing_out_after(timeout),
            None => transport,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use xcore::settings::Given;

    #[test]
    fn peppol_declares_its_settings_and_reads_through_them() {
        assert_eq!(PeppolTransport::SETTINGS.problems(), Vec::<String>::new());
        let text = |name: &str, value: &str| (name.to_string(), Given::Text(value.to_string()));
        let given = [
            text("participant", "0088:1"),
            text("partner", "0192:2"),
            text("timeout", "2s"),
        ];
        let endpoint = "https://ap.example/as4";
        let built = PeppolTransport::open(endpoint, Applies::Send, &given).expect("built");
        assert_eq!(built.me().value, "0088:1");
        assert_eq!(built.partner().value, "0192:2");
        assert_eq!(built.document.value, BILLING_INVOICE);
        assert_eq!(built.process.value, BILLING_PROCESS);
        assert_eq!(built.timeout, Some(Duration::from_secs(2)));
        let Err(refused) = PeppolTransport::open(endpoint, Applies::Receive, &given) else {
            panic!("partner is a send setting");
        };
        assert!(
            refused.message.contains("\"partner\""),
            "{}",
            refused.message
        );
    }
}
