use rand::seq::SliceRandom;
use std::sync::OnceLock;

static USER_AGENTS: OnceLock<Vec<&'static str>> = OnceLock::new();

/// Common realistic desktop user agents
fn get_user_agents() -> &'static Vec<&'static str> {
    USER_AGENTS.get_or_init(|| {
        vec![
            // Chrome on Windows
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36",

            // Chrome on Mac
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36",

            // Chrome on Linux
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",

            // Firefox on Windows
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:121.0) Gecko/20100101 Firefox/121.0",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:120.0) Gecko/20100101 Firefox/120.0",

            // Firefox on Mac
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:121.0) Gecko/20100101 Firefox/121.0",

            // Firefox on Linux
            "Mozilla/5.0 (X11; Linux x86_64; rv:121.0) Gecko/20100101 Firefox/121.0",

            // Safari on Mac
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Safari/605.1.15",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.1 Safari/605.1.15",

            // Edge on Windows
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0",
        ]
    })
}

/// Get a random user agent
pub fn random_user_agent() -> &'static str {
    let agents = get_user_agents();
    let mut rng = rand::thread_rng();
    agents.choose(&mut rng).unwrap_or(&agents[0])
}

/// Get a specific user agent for testing
pub fn default_user_agent() -> &'static str {
    get_user_agents()[0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_random_user_agent() {
        let ua = random_user_agent();
        assert!(!ua.is_empty());
        assert!(ua.contains("Mozilla"));
    }

    #[test]
    fn test_randomness() {
        let mut seen = HashSet::new();
        for _ in 0..50 {
            seen.insert(random_user_agent());
        }
        // Should get at least 3 different user agents in 50 tries
        assert!(seen.len() >= 3, "Got {} unique UAs", seen.len());
    }

    #[test]
    fn test_default_user_agent() {
        let ua = default_user_agent();
        assert!(!ua.is_empty());
    }
}
