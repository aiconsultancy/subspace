//! Subspace API query parameters as NETT's. The desktop's project filter is
//! NETT's team, a state group continues with `state`, and page sizes stay
//! within NETT's limits.

/// The largest page NETT returns for issue lists, relations and teams.
pub(crate) const MAX_PAGE: u32 = 100;
/// The largest comment, reply and pull request page.
pub(crate) const MAX_SMALL_PAGE: u32 = 50;
/// The most items NETT returns in each issue group.
pub(crate) const MAX_GROUP_PAGE: u32 = 25;

/// Renames and bounds one request's parameters for the NETT route `path`.
pub(crate) fn translate(path: &[&str], parameters: &[(String, String)]) -> Vec<(String, String)> {
    let small = matches!(
        path.last(),
        Some(&("comments" | "replies" | "pull-requests"))
    );
    let limit = if small { MAX_SMALL_PAGE } else { MAX_PAGE };
    parameters
        .iter()
        .map(|(key, value)| match key.as_str() {
            "project" => ("team".to_owned(), value.clone()),
            "stateId" => ("state".to_owned(), value.clone()),
            "planningProject" => ("project".to_owned(), value.clone()),
            "limit" => (key.clone(), bounded(value, limit)),
            "limitPerGroup" => (key.clone(), bounded(value, MAX_GROUP_PAGE)),
            _ => (key.clone(), value.clone()),
        })
        .collect()
}

fn bounded(value: &str, maximum: u32) -> String {
    value
        .parse::<u32>()
        .map_or_else(|_| value.to_owned(), |limit| limit.min(maximum).to_string())
}

#[cfg(test)]
mod tests {
    use super::translate;

    fn pairs(values: &[(&str, &str)]) -> Vec<(String, String)> {
        values
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn the_project_filter_is_the_team_and_pages_stay_within_nett_limits() {
        let list = translate(
            &["issues"],
            &pairs(&[
                ("project", "OPS"),
                ("stateId", "s1"),
                ("limit", "200"),
                ("parent", "none"),
            ]),
        );
        assert_eq!(
            list,
            pairs(&[
                ("team", "OPS"),
                ("state", "s1"),
                ("limit", "100"),
                ("parent", "none")
            ])
        );
        let groups = translate(&["issues", "groups"], &pairs(&[("limitPerGroup", "50")]));
        assert_eq!(groups, pairs(&[("limitPerGroup", "25")]));
        let comments = translate(
            &["issues", "OPS-1", "comments"],
            &pairs(&[("limit", "200")]),
        );
        assert_eq!(comments, pairs(&[("limit", "50")]));
    }
}
