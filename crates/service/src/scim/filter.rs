//! The one filter form providers send: `<attribute> eq "<value>"` (RFC 7644 §3.4.2.2, cut down).

/// A filter that isn't `<attribute> eq "<value>"` on one of the attributes it may name.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct InvalidFilter;

/// `attribute` is the one of the allowed names the filter used, as the allowed list spells it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Filter {
    pub attribute: &'static str,
    pub value: String,
}

/// Reads `filter`. The attribute and `eq` are matched without regard to case. The value is a JSON string.
pub(crate) fn parse(filter: &str, allowed: &[&'static str]) -> Result<Filter, InvalidFilter> {
    let (attribute, rest) = filter.trim().split_once(char::is_whitespace).ok_or(InvalidFilter)?;
    let (operator, value) = rest.trim_start().split_once(char::is_whitespace).ok_or(InvalidFilter)?;
    if !operator.eq_ignore_ascii_case("eq") {
        return Err(InvalidFilter);
    }
    let attribute = allowed
        .iter()
        .copied()
        .find(|name| name.eq_ignore_ascii_case(attribute))
        .ok_or(InvalidFilter)?;
    let value = serde_json::from_str::<String>(value.trim()).map_err(|_| InvalidFilter)?;
    Ok(Filter { attribute, value })
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    const USER: [&str; 2] = ["userName", "externalId"];

    #[gtest]
    fn reads_an_equality_on_an_allowed_attribute() {
        expect_that!(
            parse(r#"userName eq "ana@example.test""#, &USER),
            ok(eq(&Filter {
                attribute: "userName",
                value: "ana@example.test".to_owned()
            }))
        );
    }

    #[gtest]
    fn matches_the_attribute_and_operator_without_regard_to_case() {
        expect_that!(
            parse(r#"  EXTERNALID   EQ   "abc" "#, &USER),
            ok(eq(&Filter {
                attribute: "externalId",
                value: "abc".to_owned()
            }))
        );
    }

    #[gtest]
    fn reads_escapes_in_the_value() {
        expect_that!(
            parse(r#"userName eq "a \"b\" c""#, &USER).map(|f| f.value),
            ok(eq("a \"b\" c"))
        );
    }

    #[gtest]
    fn refuses_everything_else() {
        for filter in [
            "",
            "userName",
            "userName eq",
            "userName eq ana",
            r#"userName ne "ana""#,
            r#"userName co "ana""#,
            r#"displayName eq "ana""#,
            r#"userName eq "a" and externalId eq "b""#,
            r#"userName eq 3"#,
            r#"emails[type eq "work"] eq "a""#,
        ] {
            expect_that!(parse(filter, &USER), err(eq(&InvalidFilter)), "{filter}");
        }
    }
}
