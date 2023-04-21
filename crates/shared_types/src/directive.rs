use nom::sequence::tuple;
use nom::Parser;
use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::space0,
    combinator::value,
    error::{ContextError, ParseError},
    multi::separated_list1,
    IResult,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SrcDirective {
    Ref,
    Unref,
    Def,
    Undef,
    RuntimeRef,
    RuntimeUnref,
}
impl SrcDirective {
    pub fn parse<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
        input: &'a str,
    ) -> IResult<&'a str, SrcDirective, E> {
        alt((
            value(SrcDirective::Ref, tag("ref")),
            value(SrcDirective::Unref, tag("unref")),
            value(SrcDirective::Def, tag("def")),
            value(SrcDirective::Undef, tag("undef")),
            value(SrcDirective::RuntimeRef, tag("runtime_ref")),
            value(SrcDirective::RuntimeUnref, tag("runtime_unref")),
        ))(input)
    }
}
impl std::fmt::Display for SrcDirective {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SrcDirective::Ref => write!(f, "ref"),
            SrcDirective::Unref => write!(f, "unref"),
            SrcDirective::Def => write!(f, "def"),
            SrcDirective::Undef => write!(f, "undef"),
            SrcDirective::RuntimeRef => write!(f, "runtime_ref"),
            SrcDirective::RuntimeUnref => write!(f, "runtime_unref"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub enum ManualRefDirective {
    RuntimeRef,
    Ref,
    DataRef,
}
impl ManualRefDirective {
    pub fn parse<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
        input: &'a str,
    ) -> IResult<&'a str, ManualRefDirective, E> {
        alt((
            value(ManualRefDirective::RuntimeRef, tag("manual_runtime_ref")),
            value(ManualRefDirective::Ref, tag("manual_ref")),
            value(ManualRefDirective::DataRef, tag("data_ref")),
        ))(input)
    }
}
impl std::fmt::Display for ManualRefDirective {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManualRefDirective::RuntimeRef => write!(f, "manual_runtime_ref"),
            ManualRefDirective::Ref => write!(f, "manual_ref"),
            ManualRefDirective::DataRef => write!(f, "data_ref"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub enum AttrDirective {
    StringList,
}
impl AttrDirective {
    pub fn parse<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
        input: &'a str,
    ) -> IResult<&'a str, AttrDirective, E> {
        alt((value(AttrDirective::StringList, tag("attr.string_list")),))(input)
    }
}
impl std::fmt::Display for AttrDirective {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttrDirective::StringList => write!(f, "attr.string_list"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub enum BinaryRefDirective {
    GenerateBinary,
}
impl BinaryRefDirective {
    pub fn parse<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
        input: &'a str,
    ) -> IResult<&'a str, BinaryRefDirective, E> {
        alt((value(
            BinaryRefDirective::GenerateBinary,
            tag("binary_generate"),
        ),))(input)
    }
}
impl std::fmt::Display for BinaryRefDirective {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BinaryRefDirective::GenerateBinary => write!(f, "binary_generate"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EntityDirective {
    Link,
}

impl EntityDirective {
    pub fn parse<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
        input: &'a str,
    ) -> IResult<&'a str, EntityDirective, E> {
        alt((value(EntityDirective::Link, tag("link")),))(input)
    }
}

impl std::fmt::Display for EntityDirective {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityDirective::Link => write!(f, "link"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SrcDirectiveConfig {
    pub command: SrcDirective,
    pub act_on: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntityDirectiveConfig {
    pub command: EntityDirective,
    pub act_on: String,
    pub pointing_at: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub struct ManualRefConfig {
    pub command: ManualRefDirective,
    pub target_value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub struct AttrStringListConfig {
    pub attr_name: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct BinaryRefAndPath {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_path: Option<String>,

    pub binary_refs: BinaryRefConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub struct BinaryRefConfig {
    pub command: BinaryRefDirective,
    pub binary_name: String,
    pub target_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub enum Directive {
    SrcDirective(SrcDirectiveConfig),
    EntityDirective(EntityDirectiveConfig),
    ManualRef(ManualRefConfig),
    BinaryRef(BinaryRefConfig),
    AttrStringList(AttrStringListConfig),
}

impl Directive {
    pub fn from_strings<'a, T>(e: T) -> anyhow::Result<Vec<Directive>>
    where
        T: IntoIterator<Item = &'a String> + Copy + std::fmt::Debug,
    {
        let mut directives: Vec<Directive> = Vec::default();

        for str_directive in e.into_iter() {
            match Directive::parse::<(&str, nom::error::ErrorKind)>(str_directive.as_str()) {
                Ok((_, d)) => directives.push(d),
                Err(err) => {
                    return Err(anyhow::anyhow!(
                        "Error parsing directive {}, error: {:?}",
                        str_directive,
                        err
                    ))
                }
            }
        }
        Ok(directives)
    }

    fn entity_block<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
        input: &'a str,
    ) -> IResult<&'a str, String, E> {
        let (input, d) = nom::error::context(
            "parsing_entity_block",
            nom::bytes::complete::take_while1(|e| {
                !(e == ' ' || e == ',' || e == ':' || e == ' ' || e == '{' || e == '}')
            }),
        )(input)?;
        Ok((input, d.to_string()))
    }

    fn parse_binary_ref_directive<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
        input: &'a str,
    ) -> IResult<&'a str, Directive, E> {
        let (input, src_d) = BinaryRefDirective::parse(input)?;
        let (input, _) = nom::error::context(
            "colon after the binary command",
            tuple((space0, nom::bytes::complete::tag(":"), space0)),
        )(input)?;

        let (input, (_, nme, _)) = nom::error::context(
            "Extract target name",
            tuple((
                space0,
                nom::bytes::complete::take_while1(|e: char| !(e.is_whitespace() || e == '@')),
                space0,
            )),
        )(input)?;

        fn parse_target<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
            input: &'a str,
        ) -> IResult<&'a str, String, E> {
            let (input, _) = nom::error::context(
                "@ after target name",
                tuple((space0, nom::bytes::complete::tag("@"), space0)),
            )(input)?;

            let (input, (_, r, _)) = nom::error::context(
                "Extract value from string",
                tuple((
                    space0,
                    nom::bytes::complete::take_while1(|e: char| !e.is_whitespace()),
                    space0,
                )),
            )(input)?;
            Ok((input, r.to_string()))
        }

        let (input, maybe_target) = alt((
            parse_target.map(Some),
            tuple((space0, nom::combinator::eof)).map(|_| None),
        ))(input)?;

        let (input, _) =
            nom::error::context("Should have consumed everything", nom::combinator::eof)(input)?;

        Ok((
            input,
            Directive::BinaryRef(BinaryRefConfig {
                command: src_d,
                target_value: maybe_target,
                binary_name: nme.to_string(),
            }),
        ))
    }

    fn parse_manual_ref_directive<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
        input: &'a str,
    ) -> IResult<&'a str, Directive, E> {
        let (input, src_d) = ManualRefDirective::parse(input)?;
        let (input, _) = nom::error::context(
            "colon after entity",
            tuple((space0, nom::bytes::complete::tag(":"), space0)),
        )(input)?;

        let (input, d) = nom::error::context(
            "bazel identifier parsing",
            nom::bytes::complete::take_while1(|e| {
                !(e == ' ' || e == ',' || e == ' ' || e == '{' || e == '}')
            }),
        )(input)?;

        let (input, _) = space0(input)?;
        let (input, _) = nom::combinator::eof(input)?;

        Ok((
            input,
            Directive::ManualRef(ManualRefConfig {
                command: src_d,
                target_value: d.to_string(),
            }),
        ))
    }

    fn parse_attr_string_list_directive<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
        input: &'a str,
    ) -> IResult<&'a str, Directive, E> {
        let (input, _) = AttrDirective::parse(input)?;
        let (input, _) = nom::error::context(
            "colon after entity",
            tuple((space0, nom::bytes::complete::tag(":"), space0)),
        )(input)?;

        let (input, key) = nom::error::context(
            "non-colon",
            nom::bytes::complete::take_while1(|e| !(e == ' ' || e == ':')),
        )(input)?;

        let (input, _) = nom::error::context(
            "colon after entity",
            tuple((space0, nom::bytes::complete::tag(":"), space0)),
        )(input)?;

        let (input, value) = nom::error::context(
            "non-colon",
            nom::bytes::complete::take_while1(|e| !(e == ' ')),
        )(input)?;

        let (input, _) = space0(input)?;
        let (input, _) = nom::combinator::eof(input)?;

        Ok((
            input,
            Directive::AttrStringList(AttrStringListConfig {
                attr_name: key.to_string(),
                value: value.to_string(),
            }),
        ))
    }

    fn parse_src_directive<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
        input: &'a str,
    ) -> IResult<&'a str, Directive, E> {
        let (input, src_d) = SrcDirective::parse(input)?;
        let (input, _) = nom::error::context(
            "colon after entity",
            tuple((space0, nom::bytes::complete::tag(":"), space0)),
        )(input)?;
        let (input, src_entity) = Directive::entity_block(input)?;
        let (input, _) = space0(input)?;
        let (input, _) = nom::combinator::eof(input)?;

        Ok((
            input,
            Directive::SrcDirective(SrcDirectiveConfig {
                command: src_d,
                act_on: src_entity,
            }),
        ))
    }

    pub fn parse_entity_directive<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
        input: &'a str,
    ) -> IResult<&'a str, Directive, E> {
        // link:com.foo.bar.baz -> {org.example.Z, org.ppp.QQQ,org.eee.lll.QQQ}
        let (input, src_d) = EntityDirective::parse(input)?;
        let (input, _) = tuple((space0, nom::bytes::complete::tag(":"), space0))(input)?;
        let (input, src_entity) = Directive::entity_block(input)?;
        let (input, _) = tuple((space0, nom::bytes::complete::tag("->"), space0))(input)?;

        fn parse_target_lst<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
            input: &'a str,
        ) -> IResult<&'a str, Vec<String>, E> {
            let (input, _) = tuple((space0, nom::bytes::complete::tag("{"), space0))(input)?;
            let (input, dest_entities) = separated_list1(
                tuple((space0, nom::character::complete::char(','), space0)),
                Directive::entity_block,
            )(input)?;
            let (input, _) = tuple((space0, nom::bytes::complete::tag("}"), space0))(input)?;
            Ok((input, dest_entities))
        }

        fn parse_single<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
            input: &'a str,
        ) -> IResult<&'a str, Vec<String>, E> {
            let (input, read) = Directive::entity_block(input)?;
            Ok((input, vec![read]))
        }

        let (input, dest_entities) = alt((parse_target_lst, parse_single))(input)?;

        let (input, _) = nom::combinator::eof(input)?;

        Ok((
            input,
            Directive::EntityDirective(EntityDirectiveConfig {
                command: src_d,
                act_on: src_entity,
                pointing_at: dest_entities,
            }),
        ))
    }

    pub fn parse<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
        input: &'a str,
    ) -> IResult<&'a str, Directive, E> {
        let (input, _) = space0(input)?;
        alt((
            Directive::parse_src_directive,
            Directive::parse_entity_directive,
            Directive::parse_manual_ref_directive,
            Directive::parse_binary_ref_directive,
            Directive::parse_attr_string_list_directive,
        ))(input)
    }
}

impl std::fmt::Display for Directive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Directive::ManualRef(ManualRefConfig {
                command,
                target_value,
            }) => {
                write!(f, "{}:", command)?;
                write!(f, "{}", target_value)?;
            }
            Directive::BinaryRef(BinaryRefConfig {
                command,
                binary_name,
                target_value,
            }) => {
                write!(f, "{}:{}", command, binary_name)?;
                if let Some(v) = target_value {
                    write!(f, "{}", v)?;
                }
            }
            Directive::SrcDirective(SrcDirectiveConfig { command, act_on }) => {
                write!(f, "{}:", command)?;
                write!(f, "{}", act_on)?;
            }
            Directive::EntityDirective(EntityDirectiveConfig {
                command,
                act_on,
                pointing_at,
            }) => {
                write!(f, "{}:", command)?;
                write!(f, "{}", act_on)?;
                write!(f, " -> {{")?;
                let mut first = true;
                for e in pointing_at.iter() {
                    if !first {
                        write!(f, ",")?;
                    }
                    first = false;
                    write!(f, "{}", e)?;
                }
                write!(f, " }}")?;
            }
            Directive::AttrStringList(AttrStringListConfig { attr_name, value }) => {
                write!(f, "attr.string_list:")?;
                write!(f, "{}:", attr_name)?;
                write!(f, "{}", value)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use nom::error::ErrorKind;

    use super::*;

    fn parse_to_directive(input: &str) -> Directive {
        Directive::parse::<(&str, ErrorKind)>(input).unwrap().1
    }

    #[test]
    fn parsing() {
        assert_eq!(
            parse_to_directive(
                "link:com.foo.bar.baz -> {org.example.Z, org.ppp.QQQ,org.eee.lll.QQQ}"
            ),
            Directive::EntityDirective(EntityDirectiveConfig {
                command: EntityDirective::Link,
                act_on: "com.foo.bar.baz".to_string(),
                pointing_at: vec![
                    "org.example.Z".to_string(),
                    "org.ppp.QQQ".to_string(),
                    "org.eee.lll.QQQ".to_string(),
                ]
            })
        );

        assert_eq!(
            parse_to_directive("link:com.foo.bar.baz -> org.example.Z"),
            Directive::EntityDirective(EntityDirectiveConfig {
                command: EntityDirective::Link,
                act_on: "com.foo.bar.baz".to_string(),
                pointing_at: vec!["org.example.Z".to_string(),]
            })
        );

        assert_eq!(
            parse_to_directive("link: com.foo -> {bar.baz, oop.ee}"),
            Directive::EntityDirective(EntityDirectiveConfig {
                command: EntityDirective::Link,
                act_on: "com.foo".to_string(),
                pointing_at: vec!["bar.baz".to_string(), "oop.ee".to_string()]
            })
        );

        assert!(
            Directive::parse::<(&str, ErrorKind)>("runtime_ref:com.foo -> {bar.baz }").is_err()
        );

        assert_eq!(
            parse_to_directive("manual_runtime_ref://:build_gradle_properties_jar"),
            Directive::ManualRef(ManualRefConfig {
                command: ManualRefDirective::RuntimeRef,
                target_value: "//:build_gradle_properties_jar".to_string()
            })
        );

        assert_eq!(
            parse_to_directive("data_ref://x/y/z:artifact"),
            Directive::ManualRef(ManualRefConfig {
                command: ManualRefDirective::DataRef,
                target_value: "//x/y/z:artifact".to_string()
            })
        );

        assert_eq!(
            parse_to_directive("attr.string_list:plugins://x/y/z:artifact"),
            Directive::AttrStringList(AttrStringListConfig {
                attr_name: "plugins".to_string(),
                value: "//x/y/z:artifact".to_string()
            })
        );

        assert_eq!(
            parse_to_directive("binary_generate: my_binary"),
            Directive::BinaryRef(BinaryRefConfig {
                binary_name: "my_binary".to_string(),
                command: BinaryRefDirective::GenerateBinary,
                target_value: None
            })
        );

        assert_eq!(
            parse_to_directive("binary_generate: my_binary@ com.foo.bar.baz"),
            Directive::BinaryRef(BinaryRefConfig {
                binary_name: "my_binary".to_string(),
                command: BinaryRefDirective::GenerateBinary,
                target_value: Some("com.foo.bar.baz".to_string())
            })
        );

        assert_eq!(
            parse_to_directive("binary_generate: invoke_tool @ com.foo.bar.Baz"),
            Directive::BinaryRef(BinaryRefConfig {
                binary_name: "invoke_tool".to_string(),
                command: BinaryRefDirective::GenerateBinary,
                target_value: Some("com.foo.bar.Baz".to_string())
            })
        );
    }
    #[test]
    fn other_parsing() {
        assert_eq!(
            parse_to_directive("runtime_ref:com.example.foo.bar.baz.Elephant"),
            Directive::SrcDirective(SrcDirectiveConfig {
                command: SrcDirective::RuntimeRef,
                act_on: "com.example.foo.bar.baz.Elephant".to_string()
            })
        );
    }
}
