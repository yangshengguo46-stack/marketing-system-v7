use pretty_assertions::assert_eq;

use crate::jcs::commitment;

#[test]
fn rfc_8785_commitments_match_official_vectors() {
    let cases = [
        (
            br#" { "numbers" : [333333333.33333329, 1E30, 4.50, 2e-3, 0.000000000000000000000000001] } "#.as_slice(),
            br#"{"numbers":[333333333.3333333,1e+30,4.5,0.002,1e-27]}"#.as_slice(),
            "743e0af79012b3ec416dc1782b84f1b7af9e281d9f1dd45d861c9614595778be",
        ),
        (
            br#"{"\u20ac":"Euro Sign","\r":"Carriage Return","\ufb33":"Hebrew Letter Dalet With Dagesh","1":"One","\ud83d\ude00":"Emoji: Grinning Face","\u0080":"Control","\u00f6":"Latin Small Letter O With Diaeresis"}"#.as_slice(),
            "{\"\\r\":\"Carriage Return\",\"1\":\"One\",\"\u{80}\":\"Control\",\"ö\":\"Latin Small Letter O With Diaeresis\",\"€\":\"Euro Sign\",\"😀\":\"Emoji: Grinning Face\",\"דּ\":\"Hebrew Letter Dalet With Dagesh\"}".as_bytes(),
            "b6fe833cc17a666a21ee934f67f0c8bdd895db4d9148d996e20dd1235f805212",
        ),
        (
            br#"{"escaped":"\u000f\n\"\\/","unicode":"\u20ac"}"#.as_slice(),
            "{\"escaped\":\"\\u000f\\n\\\"\\\\/\",\"unicode\":\"€\"}".as_bytes(),
            "c72b175835616a0c8020e74e05cb372f7e3190035f7ddf6c5a0ec3b5b69632dd",
        ),
        (
            br#"{ "b" : [ true, null ], "a" : -0 }"#.as_slice(),
            br#"{"a":0,"b":[true,null]}"#.as_slice(),
            "881a59444db0c9ec28e2a870edc1cb53b10efd3f09c54c78e4916125c66685b9",
        ),
    ];

    for (raw, expected, expected_sha256) in cases {
        let actual = commitment(b"AI-IP-JCS-TEST-V1\0", raw).unwrap();
        assert_eq!(actual.canonical_bytes, expected);
        assert_eq!(actual.sha256, expected_sha256);
    }

    let appendix_b_numbers = [
        ("0", "0"),
        ("-0", "0"),
        ("5e-324", "5e-324"),
        ("-5e-324", "-5e-324"),
        ("1.7976931348623157e308", "1.7976931348623157e+308"),
        ("-1.7976931348623157e308", "-1.7976931348623157e+308"),
        ("9007199254740992", "9007199254740992"),
        ("-9007199254740992", "-9007199254740992"),
        ("295147905179352830000", "295147905179352830000"),
        ("9.999999999999997e22", "9.999999999999997e+22"),
        ("1e23", "1e+23"),
        ("1.0000000000000001e23", "1.0000000000000001e+23"),
        ("999999999999999700000", "999999999999999700000"),
        ("999999999999999900000", "999999999999999900000"),
        ("1e21", "1e+21"),
        ("9.999999999999997e-7", "9.999999999999997e-7"),
        ("0.000001", "0.000001"),
        ("333333333.3333332", "333333333.3333332"),
        ("333333333.33333325", "333333333.33333325"),
        ("333333333.3333333", "333333333.3333333"),
        ("333333333.3333334", "333333333.3333334"),
        ("333333333.33333343", "333333333.33333343"),
        ("-0.0000033333333333333333", "-0.0000033333333333333333"),
        ("1424953923781206.25", "1424953923781206.2"),
    ];
    for (raw, expected) in appendix_b_numbers {
        assert_eq!(
            commitment(b"", raw.as_bytes()).unwrap().canonical_bytes,
            expected.as_bytes()
        );
    }

    for invalid in [
        br#"{"outer":{"duplicate":1,"duplicate":2}}"#.as_slice(),
        br#"{"duplicate":1,"duplicate":2}"#.as_slice(),
        br#"{"number":NaN}"#.as_slice(),
        br#"{"number":Infinity}"#.as_slice(),
        b"{\"text\":\"\xff\"}".as_slice(),
        br#"{"text":"\ud800"}"#.as_slice(),
    ] {
        assert!(commitment(b"AI-IP-JCS-TEST-V1\0", invalid).is_err());
    }
}
