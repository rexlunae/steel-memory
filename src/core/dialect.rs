use crate::types::Drawer;

const STOP_WORDS: &[&str] = &[
    "the", "a", "an", "is", "was", "are", "were", "be", "been", "being",
    "have", "has", "had", "do", "does", "did", "will", "would", "could",
    "should", "may", "might", "shall", "can", "need", "dare", "ought",
    "used", "to", "of", "in", "for", "on", "with", "at", "by", "from",
    "as", "into", "through", "during", "before", "after", "above", "below",
    "up", "down", "out", "off", "over", "under", "again", "then", "once",
    "and", "but", "or", "nor", "so", "yet", "both", "not", "this", "that",
    "these", "those", "it", "its", "it's", "he", "she", "they", "them",
    "their", "i", "my", "we", "our", "you", "your",
];

pub fn compress_to_aaak(drawer: &Drawer) -> String {
    let emotions = detect_emotions(&drawer.content);
    let flags = detect_flags(&drawer.content);
    let entities = extract_entities(&drawer.content);
    let key_sentence = extract_key_sentence(&drawer.content);

    let topics = if !drawer.topic.is_empty() {
        drawer.topic.clone()
    } else {
        "???".to_string()
    };
    let date = if !drawer.date.is_empty() {
        drawer.date.clone()
    } else {
        "?".to_string()
    };
    let file = drawer.source_file.clone();

    let entity_str = if entities.is_empty() {
        "???".to_string()
    } else {
        entities.join("_")
    };
    let emotion_str = emotions.join(",");
    let flag_str = flags.join(",");

    format!(
        "{}|{}|{}|{}\n{}|{}|\"{}\"|{}|{}",
        drawer.wing,
        drawer.room,
        date,
        file,
        entity_str,
        topics,
        key_sentence,
        emotion_str,
        flag_str
    )
}

fn detect_emotions(content: &str) -> Vec<&'static str> {
    let lower = content.to_lowercase();
    let mut emotions = Vec::new();
    let signals: &[(&[&str], &str)] = &[
        (&["happy", "joy", "excited", "great"], "joy"),
        (&["sad", "unfortunate", "failed", "broke"], "sad"),
        (&["angry", "frustrated", "annoying"], "frus"),
        (&["worried", "concerned", "issue"], "conc"),
        (&["surprised", "unexpected"], "surp"),
        (&["decided", "chose", "switched"], "resolve"),
    ];
    for (keywords, code) in signals {
        if keywords.iter().any(|k| lower.contains(k)) {
            emotions.push(*code);
        }
    }
    emotions
}

fn detect_flags(content: &str) -> Vec<&'static str> {
    let lower = content.to_lowercase();
    let mut flags = Vec::new();
    let signals: &[(&[&str], &str)] = &[
        (&["important", "critical", "urgent"], "CRIT"),
        (&["todo", "task", "action", "implement"], "TODO"),
        (&["bug", "error", "fix"], "BUG"),
        (&["milestone", "complete", "done"], "MIL"),
        (&["decision", "decided", "chose"], "DEC"),
    ];
    for (keywords, flag) in signals {
        if keywords.iter().any(|k| lower.contains(k)) {
            flags.push(*flag);
        }
    }
    flags
}

fn extract_entities(content: &str) -> Vec<String> {
    content
        .split_whitespace()
        .filter(|w| {
            let lower = w.to_lowercase();
            let clean: String = lower.chars().filter(|c| c.is_alphabetic()).collect();
            !clean.is_empty() && !STOP_WORDS.contains(&clean.as_str()) && clean.len() > 3
        })
        .take(5)
        .map(|w| {
            w.to_lowercase()
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_')
                .collect()
        })
        .collect()
}

fn extract_key_sentence(content: &str) -> String {
    let sentence = content.split('.').next().unwrap_or(content).trim();
    if sentence.len() > 80 {
        format!("{}...", &sentence[..77])
    } else {
        sentence.to_string()
    }
}
