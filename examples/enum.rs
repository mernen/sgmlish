use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Example {
    background: Background,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Background {
    Color(String),
    Gradient { from: String, to: String },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let inputs = [
        r##"
<example>
  <background>
    <color>red</color>
  </background>
</example>
"##,
        r##"
<example>
  <background>
    <gradient from="blue" to="navy"></gradient>
  </background>
</example>
"##,
        r##"
<example>
  <background>
    <gradient>
      <from>black</from>
      <to>gold</to>
    </gradient>
  </background>
</example>
"##,
    ];
    for input in inputs {
        // Step 1: configure parser, then parse string
        let sgml = sgmlish::Parser::builder().lowercase_names().parse(input)?;
        // Step 2: normalization/validation
        let sgml = sgmlish::transforms::normalize_end_tags(sgml)?;
        // Step 3: deserialize into the desired type
        let example = sgmlish::from_fragment::<Example>(sgml)?;
        println!("{:?}", example);
    }
    Ok(())
}
