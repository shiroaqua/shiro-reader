#[path = "support/crypto.rs"]
mod support;
use support::*;

#[test]
fn passwords_use_random_salts_and_verify_correctly() {
    checks::password_hashes_use_random_salts_and_verify_real_passwords();
}

#[test]
fn issued_tokens_have_valid_signatures_claims_types_and_unique_ids() {
    checks::issued_tokens_have_valid_signatures_types_claims_and_distinct_ids();
}

#[test]
fn signatures_algorithms_and_token_structure_are_verified() {
    checks::signatures_and_algorithms_are_verified();
    checks::malformed_unsigned_or_tampered_tokens_are_rejected();
}

#[test]
fn access_and_refresh_types_cannot_be_interchanged_or_omitted() {
    checks::token_types_cannot_be_interchanged_or_omitted();
}

#[test]
fn issuer_and_audience_must_match() {
    checks::incorrect_issuer_and_audience_are_rejected_for_both_token_types();
}

#[test]
fn audience_accepts_string_arrays_but_rejects_other_member_types() {
    checks::jwt_audience_can_be_an_array_containing_the_recipient();
    checks::audience_arrays_require_only_string_members();
}

#[test]
fn required_claims_cannot_be_missing_null_or_wrong_type() {
    checks::every_application_required_claim_must_be_present();
    checks::required_claims_cannot_be_null_or_have_incorrect_types();
}

#[test]
fn expiration_and_not_before_respect_configured_clock_leeway() {
    checks::expired_and_not_yet_valid_tokens_are_rejected_without_sleep();
    checks::configured_clock_leeway_is_applied_to_exp_and_nbf();
}

#[test]
fn hs256_rejects_keys_shorter_than_256_bits() {
    checks::hs256_does_not_issue_tokens_with_keys_shorter_than_256_bits();
    checks::hs256_does_not_verify_tokens_with_keys_shorter_than_256_bits();
}

#[test]
fn hs256_accepts_a_256_bit_key() {
    checks::hs256_accepts_a_256_bit_key();
}
