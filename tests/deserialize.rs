#![cfg(feature = "serde")]

use std::str::FromStr;

use rust_decimal::Decimal;
use serde::Deserialize;
use sgmlish::parser::ParserConfig;

fn init_logger() {
    simple_logger::init().ok();
}

#[test]
fn test_auto_expansion() {
    init_logger();

    #[derive(Debug, Deserialize, PartialEq)]
    struct Test {
        attr: String,
        text: String,
        rc: String,
        c: String,
    }

    let input = r##"
        <test attr="hel<!---->lo&#33;<x></x>">
            <text>hel<!---->lo&#33;<x></x></text>
            <rc><![RCDATA[hel<!---->lo&#33;<x></x>]]></rc>
            <c><![CDATA[hel<!---->lo&#33;<x></x>]]></c>
        </test>
    "##;
    let sgml = sgmlish::parse(input).unwrap();

    let expected = Test {
        attr: "hel<!---->lo!<x></x>".to_owned(),
        text: "hello!".to_owned(),
        rc: "hel<!---->lo!<x></x>".to_owned(),
        c: "hel<!---->lo&#33;<x></x>".to_owned(),
    };
    assert_eq!(expected, sgmlish::from_fragment(sgml).unwrap());
}

#[test]
fn test_struct_dollarvalue() {
    init_logger();

    #[derive(Debug, Deserialize, PartialEq)]
    struct Test {
        #[serde(rename = "HREF")]
        href: String,
        #[serde(rename = "TARGET")]
        target: Option<String>,
        #[serde(rename = "$value")]
        text: String,
    }

    let input = r#"<A HREF="https://example.com">example<!---->!</A>"#;
    let sgml = sgmlish::parse(input).unwrap();

    let expected = Test {
        href: "https://example.com".to_owned(),
        target: None,
        text: "example!".to_string(),
    };
    assert_eq!(expected, sgmlish::from_fragment(sgml).unwrap());
}

#[test]
fn test_element_data() {
    init_logger();

    #[derive(Debug, Deserialize, PartialEq)]
    struct Item {
        #[serde(rename = "NAME")]
        name: String,
        #[serde(rename = "SOURCE")]
        source: String,
    }

    let input = r#"<ITEM><NAME>Banana</NAME><SOURCE>Store</SOURCE></ITEM>"#;
    let sgml = sgmlish::parse(input).unwrap();

    let expected = Item {
        name: "Banana".to_owned(),
        source: "Store".to_owned(),
    };
    assert_eq!(expected, sgmlish::from_fragment(sgml).unwrap());
}

/// An implementation of a tiny subset of the Open Financial Exchange (OFX) format.
///
/// Notable aspects:
///
/// * Names are case-sensitive, must be upper-case
/// * Whitespace surrounding tags is ignored
/// * Elements are either data-only (and then closing tag is optional) or "aggregates"
///   (may only directly contain whitespace or other elements; closing tag mandatory)
#[test]
fn test_ofx() {
    init_logger();

    #[derive(Debug, Deserialize, PartialEq)]
    struct BankTransactionList {
        #[serde(rename = "DTSTART")]
        dtstart: String,
        #[serde(rename = "DTEND")]
        dtend: String,
        #[serde(rename = "STMTTRN")]
        transactions: Vec<StatementTransaction>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct StatementTransaction {
        #[serde(rename = "TRNTYPE")]
        trntype: TransactionType,
        #[serde(rename = "DTPOSTED")]
        dtposted: String,
        #[serde(rename = "TRNAMT")]
        amount: Decimal,
        #[serde(rename = "FITID")]
        fitid: String,
        #[serde(rename = "MEMO")]
        memo: Option<String>,
        #[serde(rename = "CURRENCY")]
        currency: Option<Currency>,
    }

    #[derive(Debug, Deserialize, Eq, PartialEq)]
    enum TransactionType {
        #[serde(rename = "CREDIT")]
        Credit,
        #[serde(rename = "DEBIT")]
        Debit,
        #[serde(rename = "PAYMENT")]
        Payment,
        #[serde(rename = "XFER")]
        Transfer,
    }

    #[derive(Debug, Deserialize, Eq, PartialEq)]
    struct Currency {
        #[serde(rename = "CURRATE")]
        rate: Decimal,
        #[serde(rename = "CURSYM")]
        symbol: String,
    }

    let input = r##"
        <BANKTRANLIST>
            <DTSTART>20190501</DTSTART>
            <DTEND>20190531</DTEND>
            <STMTTRN>
                <TRNTYPE>PAYMENT</TRNTYPE>
                <DTPOSTED>20190510</DTPOSTED>
                <TRNAMT>-16.40</TRNAMT>
                <FITID>1DFE2867-0DE3-4357-B865-16986CBFC10A
            </STMTTRN>
            <STMTTRN>
                <TRNTYPE>PAYMENT</TRNTYPE>
                <DTPOSTED>20190512</DTPOSTED>
                <TRNAMT>-10.00</TRNAMT>
                <FITID>25518539-814F-4C2C-97E8-3B49A6D48A45
                <MEMO>Example International Payment&nbsp;
                <CURRENCY>
                    <CURRATE>1.1153
                    <CURSYM>EUR
                </CURRENCY>
            </STMTTRN>
            <STMTTRN>
                <TRNTYPE>CREDIT</TRNTYPE>
                <DTPOSTED>20190520</DTPOSTED>
                <TRNAMT>1000.00</TRNAMT>
                <FITID>3088354E-018B-41D1-ABA7-DB8F66F7B2DB
                <MEMO>PAYMENT RECEIVED
            </STMTTRN>
        </BANKTRANLIST>
    "##;

    let config = ParserConfig::builder()
        .expand_entities(|entity| match entity {
            "lt" => Some("<"),
            "gt" => Some(">"),
            "amp" => Some("&"),
            "nbsp" => Some(" "),
            _ => None,
        })
        .build();
    let sgml = sgmlish::parser::parse_with(input, &config)
        .unwrap()
        .normalize_end_tags()
        .unwrap();

    let transaction_list = sgmlish::from_fragment::<BankTransactionList>(sgml).unwrap();
    assert_eq!(transaction_list.dtstart, "20190501");
    assert_eq!(transaction_list.dtend, "20190531");
    assert_eq!(transaction_list.transactions.len(), 3);

    let trn = &transaction_list.transactions[0];
    assert_eq!(trn.trntype, TransactionType::Payment);
    assert_eq!(trn.dtposted, "20190510");
    assert_eq!(trn.amount, Decimal::from_str("-16.4").unwrap());
    assert_eq!(trn.fitid, "1DFE2867-0DE3-4357-B865-16986CBFC10A");
    assert_eq!(trn.memo, None);
    assert_eq!(trn.currency, None);

    let trn = &transaction_list.transactions[1];
    assert_eq!(trn.trntype, TransactionType::Payment);
    assert_eq!(trn.dtposted, "20190512");
    assert_eq!(trn.amount, Decimal::from(-10));
    assert_eq!(trn.fitid, "25518539-814F-4C2C-97E8-3B49A6D48A45");
    assert_eq!(trn.memo.as_deref(), Some("Example International Payment "));
    assert_eq!(
        trn.currency,
        Some(Currency {
            rate: Decimal::from_str("1.1153").unwrap(),
            symbol: "EUR".to_owned(),
        }),
    );

    let trn = &transaction_list.transactions[2];
    assert_eq!(trn.trntype, TransactionType::Credit);
    assert_eq!(trn.dtposted, "20190520");
    assert_eq!(trn.amount, Decimal::from(1_000));
    assert_eq!(trn.fitid, "3088354E-018B-41D1-ABA7-DB8F66F7B2DB");
    assert_eq!(trn.memo.as_deref(), Some("PAYMENT RECEIVED"));
    assert_eq!(trn.currency, None);
}

#[test]
fn test_html_style_boolean() {
    init_logger();

    #[derive(Debug, Deserialize, PartialEq)]
    struct Form {
        #[serde(rename = "input")]
        inputs: Vec<Input>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Input {
        #[serde(default)]
        checked: bool,
        #[serde(default)]
        disabled: bool,
    }

    let input = r##"
        <FORM>
            <INPUT checked>
            <INPUT disabled="disabled">
        </FORM>
    "##;

    let sgml = sgmlish::parse(input)
        .unwrap()
        .lowercase_identifiers()
        .normalize_end_tags()
        .unwrap();
    let form = sgmlish::from_fragment::<Form>(sgml).unwrap();

    let input1 = &form.inputs[0];
    assert!(input1.checked);
    assert!(!input1.disabled);

    let input2 = &form.inputs[1];
    assert!(!input2.checked);
    assert!(input2.disabled);
}

#[test]
fn test_complex_enum() {
    init_logger();

    #[derive(Debug, Deserialize, PartialEq)]
    struct Test {
        #[serde(rename = "item")]
        items: Vec<Item>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    enum Item {
        Unit,
        Newtype(String),
        Tuple(u8, String),
        Struct { name: String, flag: bool },
    }

    let input = r##"
        <test>
            <item><Unit></Unit></item>
            <item attr="ignored"><Newtype>test</Newtype></item>
            <item><Struct name=hello flag="true"></Struct></item>
            <item><Tuple>1</Tuple><Tuple>Two</Tuple></item>
            <item><Struct name=goodbye><flag>false</flag></Struct></item>
            <item>Unit</item>
        </test>
    "##;
    let sgml = sgmlish::parse(input).unwrap();

    let expected = Test {
        items: vec![
            Item::Unit,
            Item::Newtype("test".to_owned()),
            Item::Struct {
                name: "hello".to_owned(),
                flag: true,
            },
            Item::Tuple(1, "Two".to_owned()),
            Item::Struct {
                name: "goodbye".to_owned(),
                flag: false,
            },
            Item::Unit,
        ],
    };

    assert_eq!(expected, sgmlish::from_fragment(sgml).unwrap());
}

#[test]
fn test_complex_enum_no_containing_element() {
    init_logger();

    #[derive(Debug, Deserialize, PartialEq)]
    struct Test {
        #[serde(rename = "$value")]
        items: Vec<Item>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    enum Item {
        Unit,
        Newtype(String),
        Tuple(u8, i16),
        Struct { name: String, flag: bool },
    }

    let input = r##"
        <test>
            <Unit></Unit>
            <Newtype>test</Newtype>
            <Struct name=hello flag="true"></Struct>
            <Tuple>1</Tuple>
            <Tuple>-2</Tuple>
            <Struct flag="false"><name>goodbye</name></Struct>
            <Newtype></Newtype>
            <Unit></Unit>
        </test>
    "##;
    let sgml = sgmlish::parse(input).unwrap();

    let expected = Test {
        items: vec![
            Item::Unit,
            Item::Newtype("test".to_owned()),
            Item::Struct {
                name: "hello".to_owned(),
                flag: true,
            },
            Item::Tuple(1, -2),
            Item::Struct {
                name: "goodbye".to_owned(),
                flag: false,
            },
            Item::Newtype("".to_owned()),
            Item::Unit,
        ],
    };

    assert_eq!(expected, sgmlish::from_fragment(sgml).unwrap());
}

#[test]
fn test_sequence_of_tuples() {
    init_logger();

    #[derive(Debug, Deserialize, PartialEq)]
    struct Test {
        #[serde(rename = "n")]
        coords: Vec<(i32, i32)>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Item(i32, i32);

    let input = r##"
        <test>
            <n>1</n>
            <n>2</n>
            <n>3</n>
            <n>4</n>
            <n>5</n>
            <n>6</n>
        </test>
    "##;
    let sgml = sgmlish::parse(input).unwrap();

    let expected = Test {
        coords: vec![(1, 2), (3, 4), (5, 6)],
    };

    assert_eq!(expected, sgmlish::from_fragment(sgml).unwrap());
}
