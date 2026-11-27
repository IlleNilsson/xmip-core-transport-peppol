//! This technology's declaration: the transport guide's "The declaration",
//! `module/core/capability/transport/doc/adding-a-transport.md`. The Peppol
//! billing invoice and process are the defaults of `document` and `process`,
//! so they are declared here, and a transport carries them from here.

use xcore::settings::{Applies, Fixed, Kind, Presence, Setting, Settings};

/// The Peppol BIS Billing 3.0 invoice, the document type a transport carries
/// until `PeppolTransport::carrying`.
pub const BILLING_INVOICE: &str = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2::\
    Invoice##urn:cen.eu:en16931:2017#compliant#urn:fdc:peppol.eu:2017:poacc:billing:3.0::2.1";
/// The Peppol billing process, the one [`BILLING_INVOICE`] belongs to.
pub const BILLING_PROCESS: &str = "urn:fdc:peppol.eu:2017:poacc:billing:01:1.0";

/// Every setting a Location takes beyond its address, the access point: the
/// Party's a Send Location posts to, `peppol://ap.example/as4`, or the one a
/// Receive Location listens at.
pub const SETTINGS: &Settings = &Settings {
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
            name: "party",
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
        transport::configured::timeout(
            Applies::Both,
            None,
            "How long an access point that stops mid-message is waited on; \
             unbounded when left out.",
        ),
    ],
};

/// `peppol://`, which is `as4://` on the wire (`as4::SCHEMES`).
pub const SCHEMES: &[&str] = &["peppol"];
