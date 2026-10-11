use super::*;
fn invite() -> Invite {
    Invite { v: 1, name: "Sharing Mac".into(), platform: "macos".into(), fp: "a".repeat(64), secret: "c".repeat(64), addrs: vec!["127.0.0.1:18544".into()] }
}
#[test]
fn pake_exchanges_invite_only_for_correct_code_and_both_certificate_identities() {
    let mut host = NumericInvite::new(invite());
    let digits = host.code().to_owned();
    assert_eq!(digits.len(), 8);
    assert!(digits.bytes().all(|b| b.is_ascii_digit()));
    let client_fp = "b".repeat(64);
    let (client, request) = ClientHandshake::start(&digits, &client_fp, &invite().fp).unwrap();
    let reply = host.bootstrap(&client_fp, &request).unwrap();
    let opened = client.finish(reply).unwrap();
    assert_eq!(opened, invite());
    assert!(!host.accepts_secret(&"d".repeat(64), &opened.secret));
    assert!(host.accepts_secret(&client_fp, &opened.secret));
    let wrong = if digits == "00000000" { "11111111" } else { "00000000" };
    let (client, request) = ClientHandshake::start(wrong, &client_fp, &invite().fp).unwrap();
    assert!(client.finish(host.bootstrap(&client_fp, &request).unwrap()).is_err());
    let (client, request) = ClientHandshake::start(&digits, &client_fp, &invite().fp).unwrap();
    assert!(client.finish(host.bootstrap(&"d".repeat(64), &request).unwrap()).is_err());
}
#[test]
fn bootstrap_limits_guesses_and_expires_without_persisting_secrets() {
    let mut host = NumericInvite::new(invite());
    let client_fp = "b".repeat(64);
    for _ in 0..MAX_GUESSES {
        let (_, request) = ClientHandshake::start("12345678", &client_fp, &invite().fp).unwrap();
        assert!(host.bootstrap(&client_fp, &request).is_ok());
    }
    let (_, request) = ClientHandshake::start(host.code(), &client_fp, &invite().fp).unwrap();
    assert!(host.bootstrap(&client_fp, &request).is_err());
    host.expires = Instant::now() - Duration::from_secs(1);
    assert!(!host.accepts_secret(&client_fp, &invite().secret));
    assert!(host.bootstrap(&client_fp, &request).is_err());
}
#[test]
fn encrypted_invite_rejects_tampering_and_does_not_expose_the_code_or_secret() {
    let mut host = NumericInvite::new(invite());
    let client_fp = "b".repeat(64);
    let (client, request) = ClientHandshake::start(host.code(), &client_fp, &invite().fp).unwrap();
    let mut reply = host.bootstrap(&client_fp, &request).unwrap();
    let json = serde_json::to_string(&reply).unwrap();
    assert!(!json.contains(&invite().secret));
    // The password is never a request or response field.
    assert!(!serde_json::to_string(&request).unwrap().contains("code"));
    // Deterministically change a byte of the authenticated ciphertext.
    let first = u8::from_str_radix(&reply.sealed[..2], 16).unwrap() ^ 1;
    reply.sealed.replace_range(..2, &format!("{first:02x}"));
    assert!(client.finish(reply).is_err());
}
