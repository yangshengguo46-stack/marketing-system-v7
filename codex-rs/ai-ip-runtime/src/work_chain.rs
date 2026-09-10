use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Stage {
    #[default]
    Research,
    Direction,
    Draft,
}

impl Stage {
    fn index(self) -> usize {
        match self {
            Self::Research => 0,
            Self::Direction => 1,
            Self::Draft => 2,
        }
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StageResult {
    pub body: String,
    pub needs_review: bool,
}

/// Concrete marketing work, not a second execution engine or a quality score.
#[derive(Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkChain {
    pub brief: String,
    pub materials: Vec<String>,
    pub start_at: Stage,
    /// Research, direction, draft. Full outputs stay available for rework.
    pub results: [Option<StageResult>; 3],
}

impl WorkChain {
    pub fn new(brief: String, materials: Vec<String>) -> Self {
        Self {
            brief,
            materials,
            start_at: Stage::Research,
            results: [None, None, None],
        }
    }

    pub fn next_stage(&self) -> Option<Stage> {
        [Stage::Research, Stage::Direction, Stage::Draft]
            .into_iter()
            .find(|stage| match &self.results[stage.index()] {
                Some(result) => result.needs_review,
                None => stage.index() >= self.start_at.index(),
            })
    }

    pub fn record(&mut self, stage: Stage, body: String) {
        self.results[stage.index()] = Some(StageResult {
            body,
            needs_review: false,
        });
        for result in self.results.iter_mut().skip(stage.index() + 1).flatten() {
            result.needs_review = true;
        }
    }

    pub fn work_order(&self, work_file: &str) -> Value {
        let task = match self.next_stage() {
            Some(Stage::Research) => {
                "Read full brief/materials. Research audience situations, desired action and available proof; inspect relevant cases with sources. Separate observations from hypotheses. Explain any bridge from product to wider human concerns."
            }
            Some(Stage::Direction) => {
                "Read full brief, materials and research. Compare plausible directions; choose audience promise, IP stance, recurring themes and intended action. Explain tradeoffs and evidence gaps; do not simply copy successful accounts."
            }
            Some(Stage::Draft) => {
                "Read full brief and prior results. Write the complete shootable work: words, beats, visuals, voice/music, pacing and practical production notes aligned with IP tone, audience action and resources. Mark missing facts."
            }
            None => {
                "Review full results with the user and assemble the existing ContentPackage. Saved work is not proven marketing quality or published content. Rework only what needs changing."
            }
        };
        // Previews are navigation aids, never substitutes for the full work file.
        json!({
            "workFile": work_file,
            "nextStage": self.next_stage(),
            "task": task,
            "briefPreview": preview(&self.brief),
            "results": self.results.each_ref().map(|result| result.as_ref().map(|result| json!({
                "preview": preview(&result.body), "needsReview": result.needs_review
            })))
        })
    }
}

fn preview(text: &str) -> &str {
    &text[..text.floor_char_boundary(90)]
}

#[cfg(test)]
#[path = "work_chain_tests.rs"]
mod tests;
