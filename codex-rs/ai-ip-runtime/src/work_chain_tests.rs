use super::*;
use pretty_assertions::assert_eq;

#[test]
fn work_chain_preserves_business_inputs_and_reuses_completed_work() {
    for brief in ["黄金礼品：让客户懂得如何表达心意", "直播公会：吸引达人加入"]
    {
        let mut work = WorkChain::new(brief.into(), vec!["materials/interview.md".into()]);
        assert_eq!(work.next_stage(), Some(Stage::Research));
        work.record(Stage::Research, "访谈观察与待核实假设".into());
        work.record(Stage::Direction, "选择方向及取舍理由".into());
        work.record(Stage::Draft, "完整台词、镜头、声音、节奏与执行说明".into());
        assert_eq!(work.next_stage(), None);
        let reopened: WorkChain =
            serde_json::from_slice(&serde_json::to_vec(&work).unwrap()).unwrap();
        assert_eq!(reopened, work);
        assert_eq!(reopened.brief, brief);
        assert_eq!(reopened.materials, vec!["materials/interview.md"]);
    }
}

#[test]
fn revising_upstream_keeps_old_draft_but_requires_downstream_review() {
    let mut work = WorkChain::new("任务".into(), vec![]);
    work.record(Stage::Research, "研究一".into());
    work.record(Stage::Direction, "方向一".into());
    work.record(Stage::Draft, "旧稿".into());
    work.record(Stage::Research, "研究二".into());
    assert_eq!(work.next_stage(), Some(Stage::Direction));
    assert_eq!(
        work.results[2],
        Some(StageResult {
            body: "旧稿".into(),
            needs_review: true
        })
    );
    work.record(Stage::Direction, "已复核，仍沿用方向一".into());
    assert_eq!(work.next_stage(), Some(Stage::Draft));
    work.record(Stage::Draft, "沿用旧稿，补充声音设计".into());
    assert_eq!(work.next_stage(), None);
}

#[test]
fn existing_work_can_enter_at_draft_without_fabricating_research() {
    let mut work = WorkChain::new("只修改已有口播".into(), vec!["draft.md".into()]);
    work.start_at = Stage::Draft;
    assert_eq!(work.next_stage(), Some(Stage::Draft));
    work.record(Stage::Draft, "修改后的口播".into());
    assert_eq!(work.results[0], None);
    assert_eq!(work.results[1], None);
    assert_eq!(
        work.results[2],
        Some(StageResult {
            body: "修改后的口播".into(),
            needs_review: false
        })
    );
    assert_eq!(work.next_stage(), None);
}

#[test]
fn previews_do_not_replace_full_materials_or_split_unicode() {
    let body = "镜头、配乐、品牌气质与真实证据".repeat(100);
    let mut work = WorkChain::new("原始目标".into(), vec!["原始材料.txt".into()]);
    work.record(Stage::Research, body.clone());
    let order = work.work_order("output/marketing-work/example.json");
    assert_eq!(
        order["results"][0]["preview"],
        &body[..body.floor_char_boundary(90)]
    );
    assert_eq!(work.results[0].as_ref().unwrap().body, body);
    assert_eq!(work.materials, vec!["原始材料.txt"]);
}
