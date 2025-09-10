use std::collections::BTreeMap;

pub fn greedy_truncate_labels_prefix(labels: &mut BTreeMap<String, String>) -> String {
    let prefix = find_greedy_label_prefix(labels);

    let mut new_labels = BTreeMap::new();
    for (k, v) in labels.iter() {
        new_labels.insert(k.trim_start_matches(&prefix).to_string(), v.clone());
    }

    *labels = new_labels;

    prefix
}

pub fn greedy_truncate_labels_suffix(labels: &mut BTreeMap<String, String>) -> String {
    let prefix = find_greedy_label_suffix(labels);

    let mut new_labels = BTreeMap::new();
    for (k, v) in labels.iter() {
        new_labels.insert(k.trim_end_matches(&prefix).to_string(), v.clone());
    }

    *labels = new_labels;

    prefix
}

fn find_greedy_label_prefix(labels: &BTreeMap<String, String>) -> String {
    let Some(first) = labels.keys().next().cloned() else {
        return String::new();
    };

    labels.keys().fold(first, |common, k| {
        let mut prefix_end = 0;

        for (a, b) in common.chars().zip(k.chars()) {
            if a == b {
                prefix_end += 1;
            } else {
                break;
            }
        }

        common.chars().take(prefix_end).collect()
    })
}

fn find_greedy_label_suffix(labels: &BTreeMap<String, String>) -> String {
    let Some(first) = labels.keys().next().cloned() else {
        return String::new();
    };

    labels.keys().fold(first, |common, k| {
        let mut suffix_end = 0;

        for (a, b) in common.chars().rev().zip(k.chars().rev()) {
            if a == b {
                suffix_end += 1;
            } else {
                break;
            }
        }

        common
            .chars()
            .rev()
            .take(suffix_end)
            .collect::<String>()
            .chars()
            .rev()
            .collect()
    })
}

pub fn clean_alert_name(mut name: String) -> String {
    if name.ends_with("Trap") {
        name = name.trim_end_matches("Trap").to_string();
    }

    name
}
