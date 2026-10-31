/// Compares two dotted version strings numerically.
///
/// Absent or unparsable segments count as zero, so `14.44.17` and `14.44.17.0` compare
/// equal and `14.44.17.14` sorts above both.
pub fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let mut left_parts = left.split('.');
    let mut right_parts = right.split('.');

    loop {
        let left_part = left_parts.next();
        let right_part = right_parts.next();

        if left_part.is_none() && right_part.is_none() {
            return Ordering::Equal;
        }

        let parse = |part: Option<&str>| -> u64 {
            part.and_then(|part| part.parse::<u64>().ok()).unwrap_or(0)
        };

        match parse(left_part).cmp(&parse(right_part)) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(try_from = "String")]
pub struct VersionPrefix(String);

impl VersionPrefix {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn segments(&self) -> usize {
        self.0.split('.').count()
    }

    pub fn matches(&self, version: &str) -> bool {
        let mut actual = version.split('.');
        self.0.split('.').all(|expected| {
            actual.next().is_some_and(|part| {
                part.trim_start_matches('0') == expected.trim_start_matches('0')
            })
        })
    }
}

impl std::str::FromStr for VersionPrefix {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if !value
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit()))
        {
            return Err("version must contain numeric segments separated by dots".into());
        }
        Ok(Self(value.to_owned()))
    }
}

impl TryFrom<String> for VersionPrefix {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
