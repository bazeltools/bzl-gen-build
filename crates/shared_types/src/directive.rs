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
}
impl ManualRefDirective {
    pub fn parse<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
        input: &'a str,
    ) -> IResult<&'a str, ManualRefDirective, E> {
        alt((
            value(ManualRefDirective::RuntimeRef, tag("manual_runtime_ref")),
            value(ManualRefDirective::Ref, tag("manual_ref")),
        ))(input)
    }
}
impl std::fmt::Display for ManualRefDirective {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManualRefDirective::RuntimeRef => write!(f, "manual_runtime_ref"),
            ManualRefDirective::Ref => write!(f, "manual_runtime_ref"),
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
pub struct BinaryRefConfig {
    pub command: BinaryRefDirective,
    pub target_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
pub enum Directive {
    SrcDirective(SrcDirectiveConfig),
    EntityDirective(EntityDirectiveConfig),
    ManualRef(ManualRefConfig),
    BinaryRef(BinaryRefConfig),
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

        fn maybe_parse_target<'a, E: ParseError<&'a str> + ContextError<&'a str>>(
            input: &'a str,
        ) -> IResult<&'a str, Option<String>, E> {
            let (input, _) = nom::error::context(
                "colon after entity",
                tuple((space0, nom::bytes::complete::tag(":"), space0)),
            )(input)?;

            let (input, (_, r, _)) = tuple((
                space0,
                nom::bytes::complete::take_while1(|e: char| !e.is_whitespace()),
                space0,
            ))(input)?;
            let (input, _) = nom::combinator::eof(input)?;
            Ok((input, Some(r.to_string())))
        }

        let (input, maybe_target) = alt((
            maybe_parse_target,
            tuple((space0, nom::combinator::eof)).map(|_| None),
        ))(input)?;

        Ok((
            input,
            Directive::BinaryRef(BinaryRefConfig {
                command: src_d,
                target_value: maybe_target,
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
        alt((
            Directive::parse_src_directive,
            Directive::parse_entity_directive,
            Directive::parse_manual_ref_directive,
            Directive::parse_binary_ref_directive,
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
                target_value,
            }) => {
                write!(f, "{}", command)?;
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
            parse_to_directive("binary_generate"),
            Directive::BinaryRef(BinaryRefConfig {
                command: BinaryRefDirective::GenerateBinary,
                target_value: None
            })
        );

        assert_eq!(
            parse_to_directive("binary_generate: com.foo.bar.baz"),
            Directive::BinaryRef(BinaryRefConfig {
                command: BinaryRefDirective::GenerateBinary,
                target_value: Some("com.foo.bar.baz".to_string())
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
