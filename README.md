sgmlish
=======

[![Build status]](https://github.com/mernen/sgmlish/actions/workflows/ci.yml)
[![Version badge]](https://crates.io/crates/sgmlish)
[![Docs badge]](https://docs.rs/sgmlish)

sgmlish is a library for parsing, manipulating and deserializing SGML.

It's not intended to be a full-featured implementation of the SGML spec;
in particular, DTDs are not supported. That means case normalization and entities
must be configured before parsing, and any desired validation or normalization,
like inserting omitted tags, must be either performed through a built-in transform
or implemented manually.

Still, its support is complete enough to successfully parse SGML documents for
common applications, like [OFX] 1.x, and with little extra work it's ready to
delegate to [Serde].


## Non-goals

* **Parsing HTML.** Even though the HTML 4 spec was defined as an SGML DTD,
  browsers of that era were never close to conformant to all the intricacies of
  SGML, and websites were built with nearly zero regard for that anyway.

  Attempting to use an SGML parser to understand real-world HTML is a losing battle;
  the [HTML5 spec] was thus built with that in mind, describing how to handle
  all the ways web pages can be malformed in the best possible manner, based on
  how old browsers understood it.

  If you need to parse HTML, even old HTML, *please* use something like [html5ever].

* **Parsing XML.** This space is well-served by existing libraries like [xml-rs].
  [serde-xml-rs] offers a very similar deserialization experience to this library.

* The following SGML features are hard to properly implement without full doctype
  awareness during parsing, and are therefore currently considered beyond the
  scope of this library:
  * NET (Null End Tag) forms: `<FOO/example/`
  * Custom definitions of character sets, like `SEPCHAR` or `LCNMSTRT`


## Usage

This is a quick guide on deriving deserialization of data structures with [Serde].

First, add `sgmlish` and `serde` to your dependencies:

```toml
# Cargo.toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
sgmlish = "0.2"
```

Defining your data structures is similar to using any other Serde library:

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct Example {
  name: String,
  version: Option<String>,
}
```

Usage is typically performed in three steps:

```rust
let input = r##"
    <CRATE>
        <NAME>sgmlish
        <VERSION>0.2
    </CRATE>
"##;
// Step 1: configure parser, then parse string
let sgml = sgmlish::Parser::build()
    .lowercase_names()
    .parse(input)?;
// Step 2: normalization/validation
let sgml = sgmlish::transforms::normalize_end_tags(sgml)?;
// Step 3: deserialize into the desired type
let example = sgmlish::from_fragment::<Example>(sgml)?;
```

1.  Parsing: configure a [`sgmlish::Parser`] as desired — for example, by
    normalizing tag names or defining how entities (`&example;`) should be resolved.
    Once it's configured, feed it the SGML string.

2.  Normalization/validation: as the parser is not aware of DTDs, it does not know
    how to insert implied end tags, if those are accepted in your use case, or
    how to handle other more esoteric SGML features, like empty tags.
    *This must be fixed before proceding with deserialization.*

    A normalization transform is offered with this library: [`normalize_end_tags`].
    It assumes end tags are only omitted when the element cannot contain child
    elements. This algorithm is good enough for many SGML applications, like [OFX].

3.  Deserialization: once the event stream is normalized, pass on to Serde
    and let it do its magic.


## Interpretation when deserializing

* Primitives and strings: values can be either an attribute directly on the
  container element, or a child element with text content.

  The following are equivalent to the deserializer:

  ```xml
  <example foo="bar"></example>
  <example><foo>bar</foo></example>
  ```

* Booleans: the strings `true`, `false`, `1` and `0` are accepted,
  both as attribute values and as text content.

  In the case of attributes, HTML-style flags are also accepted:
  an empty value (explicit or implicit) and a value equal to the attribute name
  (case insensitive) are treated as `true`.

  The following all set `checked` to `true`:

  ```xml
  <example checked></example>
  <example checked=""></example>
  <example checked="1"></example>
  <example checked="checked"></example>
  <example checked="true"></example>
  <example><checked>true</checked></example>
  ```

* Structs: the tag name comes from the *parent struct*'s field, not from the value type!

  ```rust
  #[derive(Deserialize)]
  struct Root {
    // Expects a <config> element, not <MyConfiguration>
    config: MyConfiguration,
  }
  ```

  If you want to capture the text content of an element, you can make use of
  the special name `$value`:

  ```rust
  #[derive(Deserialize)]
  struct Example {
    foo: String,
    #[serde(rename = "$value")]
    content: String,
  }
  ```

  When `$value` is used, all other fields must come from attributes in the
  container element.

* Sequences: sequences are read from a contiguous series of elements
  with the same name.
  Similarly to structs, the tag name comes from the *parent struct*'s field.

  ```rust
  #[derive(Deserialize)]
  struct Example {
    // Expects a series of <host> elements, not <Hostname>
    #[serde(rename = "host")]
    hosts: Vec<Hostname>,
  }
  ```


## Crate features

* `serde` — includes support for [Serde] deserialization.

  Since this is the main use case for this library, this feature is enabled by default.
  To disable it, set `default-features = false` in your `Cargo.toml` file.


[HTML5 spec]: https://html.spec.whatwg.org/multipage/parsing.html#parsing
[html5ever]: https://lib.rs/crates/html5ever
[OFX]: https://en.wikipedia.org/wiki/Open_Financial_Exchange
[Serde]: https://serde.rs
[serde-xml-rs]: https://lib.rs/crates/serde-xml-rs
[xml-rs]: https://lib.rs/crates/xml-rs
[`sgmlish::Parser`]: https://docs.rs/sgmlish/*/sgmlish/sgmlish/parser/struct.Parser.html
[`normalize_end_tags`]: https://docs.rs/sgmlish/*/sgmlish/transforms/fn.normalize_end_tags.html

[Build status]: https://github.com/mernen/sgmlish/actions/workflows/ci.yml/badge.svg
[Version badge]: https://img.shields.io/crates/v/sgmlish.svg
[Docs badge]: https://img.shields.io/docsrs/sgmlish
