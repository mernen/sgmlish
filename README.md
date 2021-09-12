`sgmlish`
=========

This is a library for handling SGML. It's not intended to be a full-featured
implementation of the SGML spec; rather, it's meant to successfully parse
common SGML uses, and then apply a number of normalization passes to make it
suitable for deserialization, like inserting implied end tags.

In particular, DTDs are not supported. That means any desired validation or
normalization must be performed either manually or through the built-in
transforms.


## Goals

* Implementing just enough SGML to parse data you might find in the wild
  for a specific, known use case, like [OFX] 1.x, then delegating to [Serde].


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
  awareness during parsing, and are therefore beyond the scope of this library:
  * NET (Null End Tag) forms: `<FOO/example/`
  * Nested marked sections: `<![INCLUDE[ outer <![IGNORE[ inner ]]> ]]>`
  * Custom definitions of character sets, like `SEPCHAR` or `LCNMSTRT`


## Usage

This is a quick guide on deriving deserialization of data structures with [Serde].

First, add `sgmlish` and `serde` to your dependencies:

```toml
# Cargo.toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
sgmlish = "0.1"
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

Usage deviates a bit from other deserializers. The process is usually split in three phases:

```rust
let input = r##"
    <CRATE>
        <NAME>sgmlish</NAME>
        <VERSION>0.1</VERSION>
    </CRATE>
"##;
let sgml =
    // Phase 1: tokenization
    sgmlish::parse(input)?
    // Phase 2: normalization
    .trim_spaces()
    .lowercase_identifiers();
// Phase 3: deserialization
let example = sgmlish::from_fragment::<Crate>(sgml)?;
```

1.  Tokenization: `sgmlish::parse()` is invoked on an input string, producing a
    *fragment*, which is a series of *events*.

2.  Normalization: because SGML is so flexible, you'll almost certainly want to
    apply a few normalization passes to the data before deserializing.

    Some passes of interest:

    * [`trim_spaces`]: removes whitespace surrounding tags.
    * [`lowercase_identifiers`]: most SGML is case-insensitive; this will
      normalize all tag and attribute names to lowercase.
    * [`normalize_end_tags`]: inserts omitted end tags, assuming they are
      omitted only when the element cannot contain child elements.
      This algorithm is good enough for many SGML applications, like [OFX].
    * [`expand_entities`]: allows you to support `&entities;` in text content.
      No entities are supported by default, only character references (`&#32;`).
    * [`expand_marked_sections`]: processes marked sections, like `<![IGNORE[x]]>`.
      Only simple `CDATA` and `RCDATA` sections are processed by default.

    A very important rule: before proceding with deserialization, all start tags
    must have a matching end tag with identical case, in a consistent hierarchy.

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

* `deserialize` — includes support for [Serde] deserialization.

  Since this is the main use case for this library, this feature is enabled by default.


[HTML5 spec]: https://html.spec.whatwg.org/multipage/parsing.html#parsing
[html5ever]: https://lib.rs/crates/html5ever
[OFX]: https://en.wikipedia.org/wiki/Open_Financial_Exchange
[Serde]: https://serde.rs
[serde-xml-rs]: https://lib.rs/crates/serde-xml-rs
[xml-rs]: https://lib.rs/crates/xml-rs
[`expand_entities`]: https://docs.rs/sgmlish/*/sgmlish/struct.SgmlFragment.html#method.expand_entities
[`expand_marked_sections`]: https://docs.rs/sgmlish/*/sgmlish/struct.SgmlFragment.html#method.expand_marked_sections
[`lowercase_identifiers`]: https://docs.rs/sgmlish/*/sgmlish/struct.SgmlFragment.html#method.lowercase_identifiers
[`normalize_end_tags`]: https://docs.rs/sgmlish/*/sgmlish/struct.SgmlFragment.html#method.normalize_end_tags
[`trim_spaces`]: https://docs.rs/sgmlish/*/sgmlish/struct.SgmlFragment.html#method.trim_spaces
