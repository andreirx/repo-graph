# Agent notes — how to read and write them

These notes give facts about the code. They are for the agents that build and review slices. Read the notes of each crate you change.

Rules for a note:
- Write one fact in one sentence. Use simple English (ASD-STE100). Use the active voice and the present tense.
- Give the file and the line, and the commit the line is correct for. Give the source: the slice or the review that found the fact.
- Do not write a rule for agents here. Rules are in CLAUDE.md and the role prompts.
- If a note is false, report it as a finding. The manager corrects the note with a dated line.
- The manager writes the notes at each slice closeout. Builders and reviewers do not edit them during a slice.

Where the notes are: `rust/crates/<crate>/AGENT-NOTES.md`. A large module can have its own section.
