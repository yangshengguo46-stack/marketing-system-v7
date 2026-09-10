use std::io::ErrorKind;
use std::sync::Arc;

use codex_extension_api::ExtensionData;
use codex_extension_api::ExtensionRegistryBuilder;
use codex_extension_api::FunctionCallError;
use codex_extension_api::JsonToolOutput;
use codex_extension_api::ResponsesApiTool;
use codex_extension_api::ToolCall;
use codex_extension_api::ToolContributor;
use codex_extension_api::ToolExecutor;
use codex_extension_api::ToolExecutorFuture;
use codex_extension_api::ToolName;
use codex_extension_api::ToolOutput;
use codex_extension_api::ToolSpec;
use codex_extension_api::parse_tool_input_schema;
use codex_file_system::CreateDirectoryOptions;
use codex_file_system::GetMetadataOptions;
use codex_file_system::ReadFileOptions;
use codex_file_system::WriteFileOptions;
use codex_utils_path_uri::PathUri;
use serde::Deserialize;
use serde_json::json;

use crate::work_chain::Stage;
use crate::work_chain::WorkChain;

#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum Action {
    Open,
    Record,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Arguments {
    action: Action,
    work_id: String,
    environment_id: Option<String>,
    brief: Option<String>,
    materials: Option<Vec<String>>,
    start_at: Option<Stage>,
    stage: Option<Stage>,
    body: Option<String>,
}

struct MarketingWork;

/// Adds a concrete business tool to the existing native model/tool loop.
pub fn install<C: Sync + 'static>(builder: &mut ExtensionRegistryBuilder<C>) {
    builder.tool_contributor(Arc::new(MarketingWork));
}

impl ToolContributor for MarketingWork {
    fn tools(&self, _: &ExtensionData, _: &ExtensionData) -> Vec<Arc<dyn ToolExecutor<ToolCall>>> {
        vec![Arc::new(MarketingWork)]
    }
}

impl ToolExecutor<ToolCall> for MarketingWork {
    fn tool_name(&self) -> ToolName {
        ToolName::plain("marketing_work")
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec::Function(ResponsesApiTool {
            name: "marketing_work".into(),
            description: "Use for marketing/IP assignments, not general chat. Open a workId with the real brief and existing workspace-relative material paths. Read full workFile and referenced materials with native tools; results indexes 0/1/2 are research/direction/draft. Do the requested work, record its actual body, then follow the next work order. Previews are incomplete. Reopen by workId; use startAt for existing work. Rework may retain earlier drafts after review. One writer per workId.".into(),
            strict: false,
            defer_loading: None,
            output_schema: None,
            parameters: parse_tool_input_schema(&json!({
                "type":"object", "additionalProperties":false, "required":["action","workId"],
                "properties":{
                    "action":{"type":"string","enum":["open","record"]},
                    "workId":{"type":"string","pattern":"^[A-Za-z0-9_-]{1,64}$"},
                    "environmentId":{"type":"string"},
                    "brief":{"type":"string","description":"New work only; original objective and production constraints, up to 8000 UTF-8 bytes."},
                    "materials":{"type":"array","items":{"type":"string"},"maxItems":32},
                    "startAt":{"type":"string","enum":["research","direction","draft"]},
                    "stage":{"type":"string","enum":["research","direction","draft"]},
                    "body":{"type":"string","description":"Record only; actual stage output, up to 8000 UTF-8 bytes. Put larger supporting material in files."}
                }
            })).unwrap_or_else(|error| panic!("Marketing work input schema should parse: {error}")),
        })
    }

    fn handle(&self, call: ToolCall) -> ToolExecutorFuture<'_> {
        Box::pin(async move {
            let args: Arguments =
                serde_json::from_str(call.function_arguments()?).map_err(error)?;
            if args.work_id.is_empty()
                || args.work_id.len() > 64
                || !args
                    .work_id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
            {
                return Err(error(
                    "workId must contain 1–64 ASCII letters, digits, _ or -",
                ));
            }
            let environment = match args.environment_id.as_deref() {
                Some(id) => call
                    .environments
                    .iter()
                    .find(|environment| environment.environment_id == id),
                None if call.environments.len() == 1 => call.environments.first(),
                None => None,
            }
            .ok_or_else(|| error("Select an available environmentId"))?;
            let sandbox = Some(&environment.file_system_sandbox_context);
            let fs = &environment.file_system;
            let root = PathUri::from_abs_path(&environment.cwd);
            let work_file = format!("output/marketing-work/{}.json", args.work_id);
            let path = root.join(&work_file).map_err(error)?;
            let mut changed = false;
            let mut work: WorkChain = match fs
                .get_metadata(&path, GetMetadataOptions::default(), sandbox)
                .await
            {
                Ok(metadata) => {
                    if metadata.size > 256_000 {
                        return Err(error(
                            "Work file exceeds 256 KB; split supporting material into files",
                        ));
                    }
                    let bytes = fs
                        .read_file(&path, ReadFileOptions::default(), sandbox)
                        .await
                        .map_err(error)?;
                    serde_json::from_slice(&bytes).map_err(error)?
                }
                Err(err) if err.kind() == ErrorKind::NotFound && args.action == Action::Open => {
                    let brief = args
                        .brief
                        .as_deref()
                        .ok_or_else(|| error("New work requires the real brief"))?;
                    validate_text(brief)?;
                    changed = true;
                    WorkChain::new(brief.to_owned(), args.materials.clone().unwrap_or_default())
                }
                Err(err) => return Err(error(err)),
            };
            match args.action {
                Action::Open => {
                    if args.stage.is_some() || args.body.is_some() {
                        return Err(error("Use record to save a stage body"));
                    }
                    if args
                        .brief
                        .as_ref()
                        .is_some_and(|brief| brief != &work.brief)
                        || args
                            .materials
                            .as_ref()
                            .is_some_and(|materials| materials != &work.materials)
                    {
                        return Err(error(
                            "This workId already has a different brief/materials; use a new workId",
                        ));
                    }
                    if let Some(start_at) = args.start_at {
                        work.start_at = start_at;
                        changed = true;
                    }
                    if work.materials.len() > 32 {
                        return Err(error(
                            "Use at most 32 material paths, or reference a material index file",
                        ));
                    }
                    for material in &work.materials {
                        if material.len() > 512
                            || material.contains(['\\', ':'])
                            || material
                                .split('/')
                                .any(|part| matches!(part, "" | "." | ".."))
                        {
                            return Err(error(
                                "Material paths must be workspace-relative, using / separators",
                            ));
                        }
                        let material_path = root.join(material).map_err(error)?;
                        fs.get_metadata(&material_path, GetMetadataOptions::default(), sandbox)
                            .await
                            .map_err(|err| error(format!("Material {material}: {err}")))?;
                    }
                }
                Action::Record => {
                    if args.brief.is_some() || args.materials.is_some() || args.start_at.is_some() {
                        return Err(error("Record accepts stage/body, not new mission inputs"));
                    }
                    let stage = args.stage.ok_or_else(|| error("Record requires stage"))?;
                    let body = args
                        .body
                        .ok_or_else(|| error("Record requires actual body"))?;
                    validate_text(&body)?;
                    work.record(stage, body);
                    changed = true;
                }
            }
            let mut result = work.work_order(&work_file);
            let budget = call.response_byte_budget(900);
            if result.to_string().len() > budget {
                result
                    .as_object_mut()
                    .ok_or_else(|| error("Invalid work order object"))?
                    .remove("briefPreview");
                for stage in result["results"]
                    .as_array_mut()
                    .ok_or_else(|| error("Invalid work order stages"))?
                    .iter_mut()
                    .filter_map(serde_json::Value::as_object_mut)
                {
                    stage.remove("preview");
                }
            }
            if result.to_string().len() > budget {
                return Err(error("Tool output budget too small; work was not saved"));
            }
            if changed {
                fs.create_directory(
                    &root.join("output/marketing-work").map_err(error)?,
                    CreateDirectoryOptions {
                        recursive: true,
                        follow_symlinks: true,
                    },
                    sandbox,
                )
                .await
                .map_err(error)?;
                fs.write_file(
                    &path,
                    serde_json::to_vec_pretty(&work).map_err(error)?,
                    WriteFileOptions::default(),
                    sandbox,
                )
                .await
                .map_err(error)?;
            }
            Ok(
                Box::new(JsonToolOutput::new(result).with_external_context())
                    as Box<dyn ToolOutput>,
            )
        })
    }
}

fn validate_text(text: &str) -> Result<(), FunctionCallError> {
    if text.trim().is_empty() || text.len() > 8000 {
        return Err(error(
            "Supply nonempty text up to 8000 UTF-8 bytes; keep large supporting material in files",
        ));
    }
    Ok(())
}

fn error(message: impl std::fmt::Display) -> FunctionCallError {
    let mut message = message.to_string();
    message.truncate(message.floor_char_boundary(800));
    FunctionCallError::RespondToModel(message)
}

#[cfg(test)]
#[path = "work_tool_tests.rs"]
mod tests;
