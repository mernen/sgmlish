#![cfg(feature = "serde")]
// Allow unreferenced fields since we're just testing deserialization
#![allow(dead_code)]

use std::str::FromStr;

use rust_decimal::Decimal;
use serde::Deserialize;
use sgmlish::de::DeserializationError;
use sgmlish::{Parser, SgmlEvent};

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
    #[serde(rename_all = "UPPERCASE")]
    struct Test {
        href: String,
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
    #[serde(rename_all = "UPPERCASE")]
    struct Item {
        name: String,
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
fn test_ofx() -> sgmlish::Result<()> {
    init_logger();

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "UPPERCASE")]
    struct BankTransactionList {
        dtstart: String,
        dtend: String,
        #[serde(rename = "STMTTRN")]
        transactions: Vec<StatementTransaction>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "UPPERCASE")]
    struct StatementTransaction {
        trntype: TransactionType,
        dtposted: String,
        #[serde(rename = "TRNAMT")]
        amount: Decimal,
        fitid: String,
        memo: Option<String>,
        currency: Option<Currency>,
    }

    #[derive(Debug, Deserialize, Eq, PartialEq)]
    #[serde(rename_all = "UPPERCASE")]
    enum TransactionType {
        Credit,
        Debit,
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

    let sgml = Parser::builder()
        .expand_entities(|entity| match entity {
            "lt" => Some("<"),
            "gt" => Some(">"),
            "amp" => Some("&"),
            "nbsp" => Some(" "),
            _ => None,
        })
        .parse(input)?;
    let sgml = sgmlish::transforms::normalize_end_tags(sgml)?;

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

    Ok(())
}

#[test]
fn test_html_style_boolean() -> sgmlish::Result<()> {
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

    let sgml = sgmlish::Parser::builder().lowercase_names().parse(input)?;
    let sgml = sgmlish::transforms::normalize_end_tags(sgml)?;
    let form = sgmlish::from_fragment::<Form>(sgml)?;

    let input1 = &form.inputs[0];
    assert!(input1.checked);
    assert!(!input1.disabled);

    let input2 = &form.inputs[1];
    assert!(!input2.checked);
    assert!(input2.disabled);

    Ok(())
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

#[test]
fn test_reject_markup_declarations() {
    init_logger();

    #[derive(Debug, Deserialize)]
    struct Test {
        name: String,
    }

    let input = r##"
        <!DOCTYPE test>
        <test>
            <name>Testing</name>
        </test>
    "##;
    let sgml = sgmlish::parse(input).unwrap();

    let err = sgmlish::from_fragment::<Test>(sgml).unwrap_err();
    assert!(matches!(
        err,
        DeserializationError::Unsupported(SgmlEvent::MarkupDeclaration { keyword, body })
            if keyword == "DOCTYPE" && body == "test"
    ));
}

#[test]
fn test_ignore_markup_declarations() -> sgmlish::Result<()> {
    init_logger();

    #[derive(Debug, Deserialize)]
    struct Test {
        name: String,
    }

    let input = r##"
        <!DOCTYPE test>
        <test>
            <name>Testing</name>
        </test>
    "##;
    let sgml = Parser::builder()
        .ignore_markup_declarations(true)
        .parse(input)?;

    let test = sgmlish::from_fragment::<Test>(sgml)?;
    assert_eq!(test.name, "Testing");

    Ok(())
}

#[test]
fn test_reject_processing_instructions() {
    init_logger();

    #[derive(Debug, Deserialize)]
    struct Test {
        name: String,
    }

    let input = r##"
        <test>
            <?experiment>
                <name>Testing</name>
            <?/experiment>
        </test>
    "##;
    let sgml = sgmlish::parse(input).unwrap();

    let err = sgmlish::from_fragment::<Test>(sgml).unwrap_err();
    assert!(matches!(
        err,
        DeserializationError::Unsupported(SgmlEvent::ProcessingInstruction(pi)) if pi == "<?experiment>"
    ));
}

#[test]
fn test_recursive_enum() {
    init_logger();

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "lowercase")]
    pub enum Node {
        Leaf(i64),
        Branch(Box<(Node, Node)>),
    }

    let input = r##"
        <branch>
            <leaf>10</leaf>
            <branch>
                <branch>
                    <leaf>20</leaf>
                    <leaf>30</leaf>
                </branch>
                <leaf>40</leaf>
            </branch>
        </branch>
    "##;
    let sgml = sgmlish::parse(input).unwrap();
    let out = sgml.deserialize::<Node>().unwrap();
    let expected = Node::Branch(Box::new((
        Node::Leaf(10),
        Node::Branch(Box::new((
            Node::Branch(Box::new((Node::Leaf(20), Node::Leaf(30)))),
            Node::Leaf(40),
        ))),
    )));
    assert_eq!(out, expected);
}

#[test]
fn test_enum_internal_tag() {
    init_logger();

    #[derive(Debug, Deserialize)]
    pub struct Example {
        #[serde(rename = "background")]
        backgrounds: Vec<Background>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(tag = "type")]
    #[serde(rename_all = "kebab-case")]
    pub enum Background {
        Color {
            // Internal tagging does not work with newtype variants containing
            // primitive values or strings, so we must use a struct variant
            #[serde(rename = "$value")]
            value: String,
        },
        Gradient {
            from: String,
            to: String,
        },
    }

    let input = r##"
        <example>
            <background type="color">red</background>
            <background type="gradient" from="blue" to="navy"></background>
            <background type="gradient">
                <from>black</from>
                <to>gold</to>
            </background>
        </example>
    "##;
    let sgml = sgmlish::parse(input).unwrap();
    let example = sgml.deserialize::<Example>().unwrap();
    assert_eq!(example.backgrounds.len(), 3);
    assert_eq!(
        example.backgrounds[0],
        Background::Color {
            value: "red".into()
        }
    );
    assert_eq!(
        example.backgrounds[1],
        Background::Gradient {
            from: "blue".into(),
            to: "navy".into()
        }
    );
    assert_eq!(
        example.backgrounds[2],
        Background::Gradient {
            from: "black".into(),
            to: "gold".into()
        }
    );
}

#[test]
fn test_enum_untagged() {
    init_logger();

    #[derive(Debug, Deserialize)]
    pub struct Example {
        #[serde(rename = "background")]
        backgrounds: Vec<Background>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(untagged)]
    pub enum Background {
        Color(String),
        Gradient { from: String, to: String },
    }

    let input = r##"
        <example>
            <background>red</background>
            <background from="blue" to="navy"></background>
            <background>
                <from>black</from>
                <to>gold</to>
            </background>
        </example>
    "##;
    let sgml = sgmlish::parse(input).unwrap();
    let example = sgml.deserialize::<Example>().unwrap();
    assert_eq!(example.backgrounds.len(), 3);
    assert_eq!(example.backgrounds[0], Background::Color("red".into()));
    assert_eq!(
        example.backgrounds[1],
        Background::Gradient {
            from: "blue".into(),
            to: "navy".into()
        }
    );
    assert_eq!(
        example.backgrounds[2],
        Background::Gradient {
            from: "black".into(),
            to: "gold".into()
        }
    );
}
