//! Who a business document is from and for: a Peppol participant
//! identifier, and the static table that says where a participant's
//! access point is.
//!
//! A participant is `iso6523-actorid-upis::0088:7300010000001` — the one
//! scheme the network uses, two colons, and a value that is a four-digit
//! ISO 6523 International Code Designator, a colon, and the identifier the
//! designated authority issued (Peppol Policy for use of Identifiers,
//! section 4: 0088 is GS1's GLN, 0007 the Swedish organisation number,
//! 0192 the Norwegian). The scheme is fixed and the value is what is
//! checked, so a participant is written and read by its value and prints
//! qualified.
//!
//! Where a participant's access point is comes from the SMP the SML names
//! for it; until that lookup is written, a [`Directory`] is the static
//! table a Send Location resolves a participant from.

use std::fmt;

use transport::error::{Result, protocol_error};

/// The one participant identifier scheme Peppol uses.
pub const SCHEME: &str = "iso6523-actorid-upis";

/// One participant: the value under [`SCHEME`], `0088:7300010000001`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Participant {
    pub value: String,
}

impl Participant {
    /// The participant whose value is `value`: a four-digit ICD, a colon,
    /// and the issued identifier.
    ///
    /// # Errors
    /// Where the value has no ICD, an ICD that is not four digits, or
    /// nothing after the colon.
    pub fn new(value: &str) -> Result<Self> {
        let (icd, id) = value.split_once(':').ok_or_else(|| {
            protocol_error(format!(
                "a participant with no ICD before its identifier: {value}"
            ))
        })?;
        if icd.len() != 4 || !icd.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(protocol_error(format!(
                "a participant whose ICD {icd} is not four digits: {value}"
            )));
        }
        if id.is_empty() || id.contains(char::is_whitespace) {
            return Err(protocol_error(format!(
                "a participant with no identifier after its ICD: {value}"
            )));
        }
        Ok(Self {
            value: value.to_string(),
        })
    }

    /// The participant `text` names, qualified — `iso6523-actorid-upis::0088:…`
    /// — or bare — `0088:…`.
    ///
    /// # Errors
    /// Where the scheme is another than [`SCHEME`], or the value is refused
    /// by [`Self::new`].
    pub fn parse(text: &str) -> Result<Self> {
        match text.split_once("::") {
            Some((scheme, value)) if scheme == SCHEME => Self::new(value),
            Some((scheme, _)) => Err(protocol_error(format!(
                "a participant under {scheme}, and Peppol participants are {SCHEME}"
            ))),
            None => Self::new(text),
        }
    }

    /// The four-digit International Code Designator: who issued the
    /// identifier.
    #[must_use]
    pub fn icd(&self) -> &str {
        self.value.split(':').next().unwrap_or_default()
    }
}

impl fmt::Display for Participant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{SCHEME}::{}", self.value)
    }
}

/// The static table from a participant to its access point's endpoint:
/// what a Send Location resolves a participant from until SMP lookup is
/// written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Directory(Vec<(Participant, String)>);

impl Directory {
    /// This table with `participant` at `endpoint` — `https://ap.example/as4`
    /// — replacing what it had for that participant.
    #[must_use]
    pub fn with(mut self, participant: Participant, endpoint: &str) -> Self {
        self.0.retain(|(known, _)| *known != participant);
        self.0.push((participant, endpoint.to_string()));
        self
    }

    /// The endpoint of `participant`, where the table knows one.
    #[must_use]
    pub fn endpoint_of(&self, participant: &Participant) -> Option<&str> {
        self.0
            .iter()
            .find(|(known, _)| known == participant)
            .map(|(_, endpoint)| endpoint.as_str())
    }

    /// How many participants the table knows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the table knows no participant.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_participant_reads_qualified_or_bare_and_prints_qualified() {
        let qualified =
            Participant::parse("iso6523-actorid-upis::0088:7300010000001").expect("read");
        let bare = Participant::parse("0088:7300010000001").expect("read");
        assert_eq!(qualified, bare);
        assert_eq!(qualified.icd(), "0088");
        assert_eq!(qualified.value, "0088:7300010000001");
        assert_eq!(
            qualified.to_string(),
            "iso6523-actorid-upis::0088:7300010000001"
        );
        assert_eq!(
            Participant::new("0192:987654321").expect("read").icd(),
            "0192"
        );
    }

    #[test]
    fn a_participant_under_another_scheme_or_without_its_icd_is_refused() {
        for malformed in [
            "busdox-docid-qns::0088:1",
            "7300010000001",
            "088:7300010000001",
            "00x8:7300010000001",
            "0088:",
            "0088:73 00",
            "",
        ] {
            let error = Participant::parse(malformed).expect_err(malformed);
            assert!(!error.retryable, "{malformed}");
        }
        assert!(
            Participant::parse("gln::0088:1")
                .expect_err("scheme")
                .message
                .contains("Peppol participants are iso6523-actorid-upis")
        );
    }

    #[test]
    fn a_directory_answers_the_endpoint_of_a_participant_it_knows() {
        let buyer = Participant::new("0088:1").expect("buyer");
        let seller = Participant::new("0088:2").expect("seller");
        let directory = Directory::default()
            .with(buyer.clone(), "https://buyer.example/as4")
            .with(seller.clone(), "https://old.example/as4")
            .with(seller.clone(), "https://seller.example/as4");
        assert_eq!(directory.len(), 2);
        assert!(!directory.is_empty());
        assert_eq!(
            directory.endpoint_of(&buyer),
            Some("https://buyer.example/as4")
        );
        assert_eq!(
            directory.endpoint_of(&seller),
            Some("https://seller.example/as4")
        );
        assert_eq!(
            directory.endpoint_of(&Participant::new("0088:3").expect("stranger")),
            None
        );
        assert!(Directory::default().is_empty());
    }
}
