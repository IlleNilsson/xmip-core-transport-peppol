//! A keyed send carries its deduplication key as the AS4 User Message's
//! `eb:MessageId`, the same on every attempt of one Journey: the receiving
//! access point receipts the repeat again and does not deliver it again.

use std::thread;

use transport::Transport;
use xmip_core_transport_peppol::{Participant, PeppolTransport};

/// A Journey's identifier, as the runtime hands it.
const KEY: &str = "0b6f5a52-7c1e-4d0a-9a4e-3f1d2c8b9e70";

fn participant(value: &str) -> Participant {
    Participant::new(value).expect("a well-formed participant")
}

#[test]
fn a_keyed_document_is_delivered_once_however_often_it_is_sent() {
    let seller = PeppolTransport::new(
        "as4://127.0.0.1:0/as4",
        participant("0192:2"),
        participant("0088:1"),
    );
    let (listener, address) = seller.bind().expect("bound");
    let taking = thread::spawn(move || {
        (0..3)
            .map(|_| seller.accept_one(&listener).expect("receipted"))
            .collect::<Vec<_>>()
    });
    let buyer = PeppolTransport::new(
        format!("as4://{address}/as4"),
        participant("0088:1"),
        participant("0192:2"),
    );
    let invoice = b"<Invoice><ID>1</ID></Invoice>";
    buyer.send_keyed("", invoice, KEY).expect("sent");
    buyer
        .send_keyed("", invoice, KEY)
        .expect("sent again, receipted again");
    buyer.send("", invoice).expect("sent unkeyed");
    let taken = taking.join().expect("far end");
    assert!(
        taken[0]
            .as_ref()
            .is_some_and(|taken| taken.bytes == invoice)
    );
    assert_eq!(
        taken[1], None,
        "the repeat is receipted, not delivered again"
    );
    assert!(taken[2].is_some(), "an unkeyed document is delivered");
}
