//! How a Peppol Location is built from the settings it takes, declared once
//! in [`crate::declaration`] and read through (ADR-0064, amendments
//! 2026-09-26 and 2026-11-26).

use transport::Configured;
use transport::error::Result;
use xcore::settings::{Read, Settings};

use crate::{Identifier, Participant, PeppolTransport, declaration};

impl Configured for PeppolTransport {
    const SETTINGS: &'static Settings = declaration::SETTINGS;
    const SCHEMES: &'static [&'static str] = declaration::SCHEMES;

    /// The whole address: `peppol://` is `as4://` on the wire (`as4::SCHEMES`),
    /// read as the access point is.
    fn own_terms(address: &str) -> &str {
        address
    }

    /// The `party` setting: a Send Location sends to the participant it
    /// names, at the access point its address or the directory names.
    fn own_target(_address: &str) -> &str {
        transport::destination::OWN
    }

    fn timeout_kept(&mut self) -> Option<&mut dyn transport::configured::TimeoutKept> {
        Some(&mut self.timeout)
    }

    /// The signing certificate comes through the Location's credentials,
    /// never a setting; a Receive Location sends to no Party, so its own
    /// participant stands in for one.
    fn configured(address: &str, settings: &Read) -> Result<Self> {
        let me = Participant::parse(settings.text("participant"))?;
        let party = match settings.optional_text("party") {
            Some(party) => Participant::parse(party)?,
            None => me.clone(),
        };
        Ok(Self::new(address, me, party).carrying(
            Identifier::document(settings.text("document")),
            Identifier::process(settings.text("process")),
        ))
    }

    /// Each caller's turn on this participant's access point taking at
    /// most `turn`.
    fn turning_within(mut self, turn: std::time::Duration) -> Self {
        self.turn = turn;
        self
    }

    /// Each caller's turn on this participant's access point, and an HTTP/2
    /// peer's frames in a span both ways, counted on `clock`.
    fn timed_by(mut self, clock: &std::sync::Arc<dyn xcore::Clock>) -> Self {
        self.clock = std::sync::Arc::clone(clock);
        self
    }

    /// This participant's access point listening, where a Receive Location
    /// listens: the listener every receive takes from.
    fn listening(&self) -> Result<Option<&str>> {
        self.inbox().listening()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declaration::{BILLING_INVOICE, BILLING_PROCESS};
    use std::time::Duration;
    use xcore::settings::{Applies, Given};

    #[test]
    fn peppol_declares_its_settings_and_reads_through_them() {
        assert_eq!(PeppolTransport::SETTINGS.problems(), Vec::<String>::new());
        let text = |name: &str, value: &str| (name.to_string(), Given::Text(value.to_string()));
        let given = [
            text("participant", "0088:1"),
            text("party", "0192:2"),
            text("timeout", "2s"),
        ];
        let endpoint = "https://ap.example/as4";
        let built = PeppolTransport::open(endpoint, Applies::Send, &given).expect("built");
        assert_eq!(built.me().value, "0088:1");
        assert_eq!(built.party().value, "0192:2");
        assert_eq!(built.document.value, BILLING_INVOICE);
        assert_eq!(built.process.value, BILLING_PROCESS);
        assert_eq!(built.timeout, Some(Duration::from_secs(2)));
        let Err(refused) = PeppolTransport::open(endpoint, Applies::Receive, &given) else {
            panic!("party is a send setting");
        };
        assert!(refused.message.contains("\"party\""), "{}", refused.message);
    }
}
