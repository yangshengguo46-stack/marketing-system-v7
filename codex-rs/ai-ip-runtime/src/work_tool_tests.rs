use super::*;

#[test]
fn malformed_work_errors_cannot_flood_the_model_context() {
    let FunctionCallError::RespondToModel(message) = error("不能解析工作文件".repeat(2000))
    else {
        panic!("work errors must be recoverable by the model");
    };
    assert!(!message.is_empty());
    assert!(message.len() <= 800);
}
