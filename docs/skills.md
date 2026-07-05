# Option Keyboard & Skills

Agent behaviors in Kota are treated as linear combinations of markdown skill files stored in `.kota_skills/`. 

The `SkillComposer` loads active skills and weighs them according to the selected `AgentMode`:

- **Coder**: 1.0 Coder + 0.2 Eval
- **Research**: 1.0 Research + 0.3 Coder
- **Cpe**: 1.0 Cpe + 0.4 Architect
- **Architect**: 1.0 Architect + 0.5 Cpe
- **Librarian**: 1.0 Librarian + 0.4 Research
