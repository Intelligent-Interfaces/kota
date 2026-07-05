use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Option Keyboard: Skill Composer
/// Instead of loading static markdown files, we treat each skill as a basis vector (cumulant).
/// We can synthesize hybrid system prompts by applying weights to different skills.
pub struct SkillComposer {
    skills_dir: PathBuf,
}

impl SkillComposer {
    pub fn new(skills_dir: &str) -> Self {
        Self {
            skills_dir: PathBuf::from(skills_dir),
        }
    }

    /// Loads a specific skill from disk.
    fn load_skill(&self, name: &str) -> String {
        let path = self.skills_dir.join(format!("{}.md", name));
        fs::read_to_string(path).unwrap_or_default()
    }

    /// Synthesizes a new system prompt based on a linear combination of skills.
    /// For example: {"coder": 0.8, "researcher": 0.2}
    pub fn compose(&self, weights: &HashMap<&str, f32>) -> String {
        let base = self.load_skill("base");
        if base.is_empty() {
            return "You are Kota.".to_string();
        }

        let mut composed_prompt = format!("{}\n\n=== SYNTHESIZED SKILLS ===\n", base);

        // Sort by weight descending so the most heavily weighted skills appear first
        let mut sorted_weights: Vec<(&&str, &f32)> = weights.iter().collect();
        sorted_weights.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());

        for (skill_name, weight) in sorted_weights {
            if *weight > 0.0 {
                let skill_content = self.load_skill(skill_name);
                if !skill_content.is_empty() {
                    composed_prompt.push_str(&format!(
                        "\n--- SKILL: {} (Weight: {:.2}) ---\n{}\n",
                        skill_name, weight, skill_content
                    ));
                }
            }
        }

        composed_prompt
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;

    #[test]
    fn test_compose_skills() {
        let temp_dir = std::env::temp_dir().join("kota_test_skills");
        let _ = fs::create_dir_all(&temp_dir);

        // Create mock skill files
        let mut base_file = File::create(temp_dir.join("base.md")).unwrap();
        writeln!(base_file, "Base Instructions").unwrap();

        let mut coder_file = File::create(temp_dir.join("coder.md")).unwrap();
        writeln!(coder_file, "Coder Instructions").unwrap();

        let composer = SkillComposer::new(temp_dir.to_str().unwrap());

        let mut weights = HashMap::new();
        weights.insert("coder", 1.0);

        let prompt = composer.compose(&weights);
        assert!(prompt.contains("Base Instructions"));
        assert!(prompt.contains("--- SKILL: coder (Weight: 1.00) ---"));
        assert!(prompt.contains("Coder Instructions"));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
