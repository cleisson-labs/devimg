use super::{
    agent_instruction_commands, agent_instruction_files, agent_task_generated_artifacts,
    agent_task_manifest_path, blocking_suggestion_count, build_openai_alt_body,
    build_openai_review_body, call_openai_alt_with_transport, call_openai_draft_with_transport,
    call_openai_responses_api_with_transport, call_openai_review_with_transport, check_command,
    comma_or_none, command_agent, command_agent_init, command_agent_task, command_ai,
    command_ai_consent, command_alt, command_alt_with_openai, command_check, command_compare,
    command_doctor, command_draft, command_draft_with_openai, command_init, command_inspect,
    command_manifest, command_manifest_export, command_optimize, command_report, command_review,
    command_review_ai, command_review_ai_with_openai, command_suggest, comparable_output_path,
    config_option, current_dir_error, doctor_command, extract_openai_output_text, init_command,
    is_protected_agent_instruction_path, manifest_export_write_command, normalize_lexical,
    openai_alt_text_format, openai_alt_user_text, openai_draft_text_format, openai_draft_user_text,
    openai_image_detail, openai_review_text_format, openai_review_user_text,
    openai_transport_error_message, optimize_command, parse_env_line_value, push_diagnostics,
    push_named_paths, read_ai_provider_key, read_env_file_value, render_agent_task,
    render_core_error, render_suggest_terminal_summary, review_asset_path_prefix,
    review_has_ai_only_args, review_project_root, shell_arg, starter_config,
    validate_distinct_alt_outputs, validate_distinct_review_outputs, with_check_hint,
    write_text_file, AgentArgs, AgentCommand, AgentInitArgs, AgentTarget, AgentTaskAgent,
    AgentTaskArgs, AgentTaskGeneratedArtifacts, AiArgs, AiCommand, AiConsentArgs, AiImageDetailArg,
    AltArgs, CheckArgs, CliError, CompareArgs, DoctorArgs, DraftArgs, DraftTypeArg, InitArgs,
    InitProfile, InspectArgs, ManifestArgs, ManifestCommand, ManifestExportArgs,
    ManifestExportFormat, OpenAiResponsesTransport, OpenAiTransportError, OptimizeArgs, ReportArgs,
    ReviewArgs, SuggestArgs, SuggestFailSeverity,
};
use devimg_core::{
    AiAltImageInput, AiAltManifestSummary, AiAltProviderPayload, AiAltRequest, AiAltSource,
    AiAltVariant, AiDraftOptions, AiDraftProviderPayload, AiDraftRequest, AiDraftType, AiProvider,
    AiReviewImageInput, AiReviewManifestSummary, AiReviewOutput, AiReviewProviderPayload,
    AiReviewRequest, Config, DoctorBudget, DoctorCheck, DoctorDiagnostic, DoctorReport, Manifest,
    ManifestOutput, ProjectConfig, SourceConfig, SuggestionItem, SuggestionReport,
    SuggestionSummary,
};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn env_line_parser_extracts_key_without_comments() {
    assert_eq!(
        parse_env_line_value("OPENAI_API_KEY=test-secret # local", "OPENAI_API_KEY").as_deref(),
        Some("test-secret")
    );
    assert_eq!(
        parse_env_line_value("export OPENAI_API_KEY=\"quoted secret\"", "OPENAI_API_KEY")
            .as_deref(),
        Some("quoted secret")
    );
    assert_eq!(
        parse_env_line_value("ANTHROPIC_API_KEY=other", "OPENAI_API_KEY"),
        None
    );
}

#[test]
fn openai_response_text_extraction_supports_responses_shape() {
    let value = json!({
        "output": [
            {
                "type": "message",
                "content": [
                    {
                        "type": "output_text",
                        "text": "{\"summary\":\"ok\",\"observations\":[]}"
                    }
                ]
            }
        ]
    });

    assert_eq!(
        extract_openai_output_text(&value).as_deref(),
        Some("{\"summary\":\"ok\",\"observations\":[]}")
    );
}

#[test]
fn env_line_parser_handles_quotes_comments_and_non_matches() {
    assert_eq!(parse_env_line_value("", "OPENAI_API_KEY"), None);
    assert_eq!(parse_env_line_value("   # comment", "OPENAI_API_KEY"), None);
    assert_eq!(
        parse_env_line_value("OPENAI_API_KEY", "OPENAI_API_KEY"),
        None
    );
    assert_eq!(
        parse_env_line_value("OPENAI_API_KEY='quoted # secret'", "OPENAI_API_KEY").as_deref(),
        Some("quoted # secret")
    );
    assert_eq!(
        parse_env_line_value(" export OPENAI_API_KEY = spaced ", "OPENAI_API_KEY").as_deref(),
        Some("spaced")
    );
}

#[test]
fn env_file_reader_handles_missing_empty_and_present_values() {
    let dir = temp_dir("cli_env_file_reader");
    let missing = dir.join(".env.missing");
    assert!(read_env_file_value(&missing, "OPENAI_API_KEY")
        .expect("missing env file is ok")
        .is_none());

    let env_file = dir.join(".env");
    fs::write(
        &env_file,
        "OPENAI_API_KEY=\nOPENAI_API_KEY=from-file # local\nANTHROPIC_API_KEY=other\n",
    )
    .expect("write env fixture");
    assert_eq!(
        read_env_file_value(&env_file, "OPENAI_API_KEY")
            .expect("env file reads")
            .as_deref(),
        Some("from-file")
    );
    assert!(read_env_file_value(&env_file, "MISSING_KEY")
        .expect("env file reads")
        .is_none());
    cleanup(&dir);
}

#[test]
fn output_path_validation_rejects_collisions() {
    let dir = temp_dir("cli_output_validation");
    let manifest = dir.join("manifest.json");
    let html = dir.join("review.html");
    let ai = dir.join("ai.json");
    let markdown = dir.join("review.md");

    assert!(validate_distinct_review_outputs(&manifest, Some(&html), &ai, Some(&markdown)).is_ok());
    assert_config_error(validate_distinct_review_outputs(
        &manifest,
        Some(&ai),
        &ai,
        Some(&markdown),
    ));
    assert_config_error(validate_distinct_review_outputs(
        &manifest,
        Some(&html),
        &ai,
        Some(&ai),
    ));
    assert_config_error(validate_distinct_review_outputs(
        &manifest,
        Some(&html),
        &ai,
        Some(&html),
    ));

    assert!(validate_distinct_alt_outputs(&manifest, &ai, Some(&markdown)).is_ok());
    assert_config_error(validate_distinct_alt_outputs(&manifest, &ai, Some(&ai)));

    let relative = comparable_output_path(Path::new("target/../target/file.txt"))
        .expect("relative path normalizes");
    assert!(relative.ends_with("target/file.txt"));
    cleanup(&dir);
}

#[test]
fn review_ai_only_arg_detection_covers_each_ai_flag() {
    let mut args = review_args();
    assert!(!review_has_ai_only_args(&args));
    args.ai_provider = Some(super::AiProviderArg::Openai);
    assert!(review_has_ai_only_args(&args));

    let mut args = review_args();
    args.model = Some("gpt-test".to_string());
    assert!(review_has_ai_only_args(&args));

    let mut args = review_args();
    args.metadata_only = true;
    assert!(review_has_ai_only_args(&args));

    let mut args = review_args();
    args.include_images = true;
    assert!(review_has_ai_only_args(&args));

    let mut args = review_args();
    args.dry_run = true;
    assert!(review_has_ai_only_args(&args));

    let mut args = review_args();
    args.ai_output = Some(PathBuf::from("ai.json"));
    assert!(review_has_ai_only_args(&args));

    let mut args = review_args();
    args.markdown = Some(PathBuf::from("ai.md"));
    assert!(review_has_ai_only_args(&args));

    let mut args = review_args();
    args.max_images = 1;
    assert!(review_has_ai_only_args(&args));

    let mut args = review_args();
    args.image_detail = AiImageDetailArg::High;
    assert!(review_has_ai_only_args(&args));
}

#[test]
fn openai_body_builders_include_metadata_schemas_and_image_bytes() {
    let dir = temp_dir("cli_openai_body_builders");
    fs::create_dir_all(dir.join("generated")).expect("create generated dir");
    fs::write(dir.join("generated/review.webp"), b"review image").expect("write review image");
    fs::write(dir.join("generated/alt.png"), b"alt image").expect("write alt image");

    let review = review_request(true);
    let review_body = super::build_openai_review_body(&review, &dir).expect("review body builds");
    assert_eq!(review_body["model"], "gpt-test");
    assert_eq!(review_body["text"]["format"]["name"], "devimg_ai_review");
    let review_content = review_body["input"][1]["content"]
        .as_array()
        .expect("review content array");
    assert_eq!(review_content[0]["type"], "input_text");
    assert!(review_content[1]["image_url"]
        .as_str()
        .expect("review image url")
        .starts_with("data:image/webp;base64,"));
    assert_eq!(review_content[1]["detail"], "high");

    let alt = alt_request(true);
    let alt_body = super::build_openai_alt_body(&alt, &dir).expect("alt body builds");
    assert_eq!(alt_body["model"], "gpt-alt");
    assert_eq!(alt_body["text"]["format"]["name"], "devimg_alt_text");
    let alt_content = alt_body["input"][1]["content"]
        .as_array()
        .expect("alt content array");
    assert!(alt_content[1]["image_url"]
        .as_str()
        .expect("alt image url")
        .starts_with("data:image/png;base64,"));
    assert_eq!(alt_content[1]["detail"], "auto");

    let draft = draft_request();
    let draft_body = super::build_openai_draft_body(&draft);
    assert_eq!(draft_body["model"], "gpt-draft");
    assert_eq!(draft_body["text"]["format"]["name"], "devimg_draft");
    assert!(openai_review_user_text(&review).contains("Review this DevImg manifest metadata"));
    assert!(openai_alt_user_text(&alt).contains("Draft alt text"));
    assert!(openai_draft_user_text(&draft).contains("release-notes"));
    assert_eq!(openai_image_detail("original"), "high");
    assert_eq!(openai_image_detail("unknown"), "low");
    assert_eq!(
        openai_review_text_format()["schema"]["required"][0],
        "summary"
    );
    assert_eq!(openai_alt_text_format()["schema"]["required"][1], "drafts");
    assert_eq!(
        openai_draft_text_format()["schema"]["required"][1],
        "sections"
    );

    assert_io_error(build_openai_review_body(
        &review_request(true),
        &dir.join("missing-root"),
    ));
    assert_io_error(build_openai_alt_body(
        &alt_request(true),
        &dir.join("missing-root"),
    ));

    cleanup(&dir);
}

#[test]
fn openai_fake_transport_covers_success_schema_and_error_mapping() {
    let review_payload = json!({
        "summary": "ok",
        "observations": [
            {
                "category": "format-quality concern",
                "severity": "advisory",
                "source_path": "assets/photo.png",
                "preset": "card",
                "output_path": "generated/review.webp",
                "rationale": "review",
                "suggested_next_command": "devimg review"
            }
        ]
    });
    let transport = FakeTransport::ok(output_text(&review_payload.to_string()));
    let review = call_openai_review_with_transport(
        &review_request(false),
        Path::new("."),
        "secret-key",
        &transport,
    )
    .expect("review transport parses");
    assert_eq!(review.summary, "ok");
    assert_eq!(transport.seen.borrow()[0].0, "secret-key");
    assert_eq!(transport.seen.borrow()[0].1["model"], "gpt-test");

    let alt_payload = json!({
        "summary": "alt ok",
        "drafts": [
            {
                "source_path": "assets/photo.png",
                "representative_image_path": "generated/alt.png",
                "candidate_alt_text": "Alt text",
                "review_note": "Review",
                "confidence": "high",
                "image_category": "content-photo",
                "warnings": []
            }
        ]
    });
    let alt = call_openai_alt_with_transport(
        &alt_request(false),
        Path::new("."),
        "secret-key",
        &FakeTransport::ok(output_text(&alt_payload.to_string())),
    )
    .expect("alt transport parses");
    assert_eq!(alt.summary, "alt ok");

    let draft_payload = json!({
        "summary": "draft ok",
        "sections": [
            {"heading": "Release", "body": "Body", "bullets": ["one"]}
        ]
    });
    let draft = call_openai_draft_with_transport(
        &draft_request(),
        "secret-key",
        &FakeTransport::ok(output_text(&draft_payload.to_string())),
    )
    .expect("draft transport parses");
    assert_eq!(draft.sections[0].heading, "Release");

    assert_config_error(call_openai_review_with_transport(
        &review_request(false),
        Path::new("."),
        "secret-key",
        &FakeTransport::ok(output_text("not json")),
    ));
    assert_config_error(call_openai_alt_with_transport(
        &alt_request(false),
        Path::new("."),
        "secret-key",
        &FakeTransport::ok(output_text("not json")),
    ));
    assert_config_error(call_openai_draft_with_transport(
        &draft_request(),
        "secret-key",
        &FakeTransport::ok(output_text("not json")),
    ));

    assert_config_error(call_openai_responses_api_with_transport(
        Path::new("devimg.toml"),
        "secret-key",
        &json!({"model": "gpt-test"}),
        "missing output",
        &FakeTransport::ok(json!({"output": []})),
    ));
    for error in [
        OpenAiTransportError::BuildClient,
        OpenAiTransportError::Request,
        OpenAiTransportError::Status(429),
        OpenAiTransportError::InvalidJson,
    ] {
        assert_config_error(call_openai_responses_api_with_transport(
            Path::new("devimg.toml"),
            "secret-key",
            &json!({"model": "gpt-test"}),
            "missing output",
            &FakeTransport::err(error),
        ));
        assert!(!openai_transport_error_message(error).is_empty());
    }
}

#[test]
fn openai_output_text_extraction_handles_flat_concatenated_and_missing_text() {
    assert_eq!(
        extract_openai_output_text(&json!({"output_text": "flat"})).as_deref(),
        Some("flat")
    );
    assert_eq!(
        extract_openai_output_text(&json!({
            "output": [
                {"content": [{"text": "hello "}, {"text": "world"}]},
                {"type": "tool_call"},
                {"content": [{"type": "ignored"}]}
            ]
        }))
        .as_deref(),
        Some("hello world")
    );
    assert!(extract_openai_output_text(&json!({"output": []})).is_none());
    assert!(extract_openai_output_text(&json!({})).is_none());
}

#[test]
fn command_rendering_and_agent_instruction_helpers_cover_quoting_branches() {
    assert_eq!(init_command(Path::new("devimg.toml")), "devimg init");
    assert_eq!(
        init_command(Path::new("custom config.toml")),
        "devimg init --config 'custom config.toml'"
    );
    assert_eq!(
        optimize_command(Path::new("devimg.toml")),
        "devimg optimize --allow-overwrite"
    );
    assert_eq!(doctor_command(Path::new("devimg.toml")), "devimg doctor");
    assert_eq!(check_command(Path::new("devimg.toml")), "devimg check");
    assert_eq!(config_option(Path::new("devimg.toml")), "");
    assert_eq!(shell_arg("plain/path-1"), "plain/path-1");
    assert_eq!(shell_arg("two words"), "'two words'");
    assert_eq!(shell_arg("it's"), "'it'\\''s'");

    let export = ManifestExportArgs {
        manifest: PathBuf::from("public/images/devimg-manifest.json"),
        format: ManifestExportFormat::Typescript,
        output: Some(PathBuf::from("lib/devimg generated.ts")),
        strip_prefix: Some("public assets".to_string()),
        url_prefix: "/".to_string(),
        check: false,
        typescript_helpers: true,
    };
    let command = manifest_export_write_command(&export, export.output.as_deref().expect("output"));
    assert!(command.contains("--format typescript"));
    assert!(command.contains("--strip-prefix 'public assets'"));
    assert!(command.contains("--typescript-helpers"));

    let default_commands = agent_instruction_commands(Path::new("devimg.toml"));
    assert!(default_commands
        .config_note
        .contains("DevImg uses `devimg.toml` by default"));
    let custom_commands = agent_instruction_commands(Path::new("configs/images.toml"));
    assert!(custom_commands.config_note.contains("configs/images.toml"));

    for target in [AgentTarget::Codex, AgentTarget::Claude, AgentTarget::Both] {
        let files = agent_instruction_files(&AgentInitArgs {
            target,
            output_dir: PathBuf::from("."),
            config: PathBuf::from("devimg.toml"),
            stdout: false,
            force: false,
        });
        assert!(!files.is_empty());
    }
}

#[test]
fn review_asset_prefix_handles_nested_sibling_and_external_paths() {
    let dir = temp_dir("cli_review_asset_prefix");
    let project = dir.join("project");
    let nested = project.join(".devimg/review.html");
    let root_output = project.join("review.html");
    let outside = dir.join("outside/review.html");
    fs::create_dir_all(nested.parent().expect("nested parent")).expect("nested dirs");
    fs::create_dir_all(outside.parent().expect("outside parent")).expect("outside dirs");

    assert_eq!(
        review_asset_path_prefix(&root_output, &project).unwrap(),
        ""
    );
    assert_eq!(review_asset_path_prefix(&nested, &project).unwrap(), "..");
    assert_eq!(
        review_asset_path_prefix(&outside, &project).unwrap(),
        project.to_string_lossy().replace('\\', "/")
    );
    assert_eq!(
        normalize_lexical(Path::new("a/./b/../c")),
        PathBuf::from("a/c")
    );
    cleanup(&dir);
}

#[test]
fn command_validation_branches_fail_before_network_or_provider_calls() {
    let dir = temp_dir("cli_command_validation");
    let config = dir.join("devimg.toml");
    write_minimal_config(&config);

    assert_config_error(command_review(review_args_with(|args| {
        args.model = Some("gpt-test".to_string());
    })));
    assert_config_error(command_review(review_args_with(|args| {
        args.output = None;
        args.stdout = false;
    })));
    assert_config_error(command_review(review_args_with(|args| {
        args.stdout = true;
        args.output = Some(PathBuf::from("review.html"));
    })));

    assert_config_error(command_review_ai(review_args_with(|args| {
        args.ai = true;
        args.stdout = true;
    })));
    assert_config_error(command_review_ai(review_args_with(|args| {
        args.ai = true;
        args.metadata_only = true;
        args.include_images = true;
    })));
    assert_config_error(command_review_ai(review_args_with(|args| {
        args.ai = true;
        args.max_images = 0;
    })));
    assert_config_error(command_review_ai(review_args_with(|args| {
        args.ai = true;
    })));
    assert_config_error(command_review_ai(review_args_with(|args| {
        args.ai = true;
        args.ai_provider = Some(super::AiProviderArg::Anthropic);
    })));
    assert_config_error(command_review_ai(review_args_with(|args| {
        args.ai = true;
        args.ai_provider = Some(super::AiProviderArg::Openai);
    })));
    assert_config_error(command_review_ai(review_args_with(|args| {
        args.ai = true;
        args.ai_provider = Some(super::AiProviderArg::Openai);
        args.model = Some(" ".to_string());
    })));

    assert_config_error(command_alt(alt_args_with(&config, |args| {
        args.model = Some(" ".to_string());
    })));
    assert_config_error(command_alt(alt_args_with(&config, |args| {
        args.metadata_only = true;
        args.include_images = true;
    })));
    assert_config_error(command_alt(alt_args_with(&config, |args| {
        args.max_images = 0;
    })));
    assert_config_error(command_alt(alt_args_with(&config, |args| {
        args.model = Some("gpt-test".to_string());
    })));
    assert_config_error(command_alt(alt_args_with(&config, |args| {
        args.ai_provider = Some(super::AiProviderArg::Openai);
    })));
    assert_config_error(command_alt(alt_args_with(&config, |args| {
        args.include_images = true;
        args.ai_provider = Some(super::AiProviderArg::Anthropic);
        args.model = Some("claude-test".to_string());
    })));
    assert_config_error(command_alt(alt_args_with(&config, |args| {
        args.include_images = true;
    })));

    assert_config_error(command_draft(draft_args_with(&config, |args| {
        args.metadata_only = true;
        args.ai_provider = Some(super::AiProviderArg::Openai);
    })));
    assert_config_error(command_draft(draft_args_with(&config, |args| {
        args.model = Some(" ".to_string());
    })));
    assert_config_error(command_draft(draft_args_with(&config, |args| {
        args.ai_provider = Some(super::AiProviderArg::Openai);
    })));
    assert_config_error(command_draft(draft_args_with(&config, |args| {
        args.model = Some("gpt-test".to_string());
    })));

    assert_config_error(command_ai_consent(AiConsentArgs {
        config: config.clone(),
        ai_provider: super::AiProviderArg::Openai,
        model: " ".to_string(),
        metadata_only: false,
        include_images: false,
        dry_run: true,
        output: None,
        force: false,
    }));
    assert_config_error(command_ai_consent(AiConsentArgs {
        config: config.clone(),
        ai_provider: super::AiProviderArg::Openai,
        model: "gpt-test".to_string(),
        metadata_only: true,
        include_images: true,
        dry_run: true,
        output: None,
        force: false,
    }));

    assert_config_error(command_doctor(DoctorArgs {
        config: config.clone(),
        json: false,
        export_output: None,
        export_format: ManifestExportFormat::Json,
        strip_prefix: None,
        url_prefix: String::new(),
        typescript_helpers: true,
    }));
    assert_config_error(command_doctor(DoctorArgs {
        config: config.clone(),
        json: false,
        export_output: Some(dir.join("helper.json")),
        export_format: ManifestExportFormat::Json,
        strip_prefix: None,
        url_prefix: String::new(),
        typescript_helpers: true,
    }));

    cleanup(&dir);
}

#[test]
fn command_file_branches_write_nested_outputs_and_wrapper_commands_dispatch() {
    let dir = temp_dir("cli_command_files");
    let nested_config = dir.join("nested/devimg.toml");
    command_init(InitArgs {
        config: nested_config.clone(),
        profile: Some(super::InitProfile::Vite),
        stdout: false,
        force: false,
    })
    .expect("init writes nested config");
    assert!(nested_config.exists());

    let manifest_path = dir.join("manifest.json");
    write_manifest_fixture(&manifest_path, "devimg.toml");
    command_report(ReportArgs {
        manifest: manifest_path.clone(),
    })
    .expect("report renders");

    let html_output = dir.join("review/review.html");
    command_review(ReviewArgs {
        manifest: manifest_path.clone(),
        output: Some(html_output.clone()),
        stdout: false,
        force: false,
        ai: false,
        ai_provider: None,
        model: None,
        metadata_only: false,
        include_images: false,
        dry_run: false,
        ai_output: None,
        markdown: None,
        max_images: super::DEFAULT_AI_REVIEW_MAX_IMAGES,
        image_detail: AiImageDetailArg::Low,
    })
    .expect("review writes nested html");
    assert!(html_output.exists());

    let export_path = dir.join("exports/devimg.generated.json");
    command_manifest(ManifestArgs {
        command: ManifestCommand::Export(ManifestExportArgs {
            manifest: manifest_path.clone(),
            format: ManifestExportFormat::Json,
            output: Some(export_path.clone()),
            strip_prefix: None,
            url_prefix: String::new(),
            check: false,
            typescript_helpers: false,
        }),
    })
    .expect("manifest wrapper dispatches export");
    assert!(export_path.exists());

    let agent_dir = dir.join("agent");
    command_agent(AgentArgs {
        command: AgentCommand::Init(AgentInitArgs {
            target: AgentTarget::Claude,
            output_dir: agent_dir.clone(),
            config: PathBuf::from("custom config.toml"),
            stdout: false,
            force: false,
        }),
    })
    .expect("agent wrapper dispatches init");
    assert!(agent_dir.join("CLAUDE.md").exists());

    let config = dir.join("devimg.toml");
    write_minimal_config(&config);
    fs::create_dir_all(dir.join("assets/images")).expect("create source dir");
    command_ai(AiArgs {
        command: AiCommand::Consent(AiConsentArgs {
            config: config.clone(),
            ai_provider: super::AiProviderArg::Openai,
            model: "gpt-test".to_string(),
            metadata_only: false,
            include_images: false,
            dry_run: true,
            output: Some(dir.join("ai/consent.json")),
            force: false,
        }),
    })
    .expect("ai wrapper dispatches consent");
    assert!(dir.join("ai/consent.json").exists());

    cleanup(&dir);
}

#[test]
fn provider_command_paths_use_injected_openai_clients_without_network() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let dir = temp_dir("cli_provider_command_paths");
    let config = dir.join("devimg.toml");
    write_minimal_config(&config);
    fs::create_dir_all(dir.join("assets/images")).expect("source dir");
    fs::create_dir_all(dir.join("public/images")).expect("manifest dir");
    write_manifest_fixture(
        &dir.join("public/images/devimg-manifest.json"),
        &config.to_string_lossy(),
    );

    let previous_openai = std::env::var("OPENAI_API_KEY").ok();
    std::env::set_var("OPENAI_API_KEY", "unit-test-key");

    let mut openai_review = |request: &AiReviewRequest, project_root: &Path, key: &str| {
        assert_eq!(key, "unit-test-key");
        assert_eq!(project_root, dir.as_path());
        assert_eq!(request.model, "gpt-test");
        Ok(AiReviewProviderPayload {
            summary: "review ok".to_string(),
            observations: Vec::new(),
        })
    };

    command_review_ai_with_openai(
        ReviewArgs {
            manifest: dir.join("public/images/devimg-manifest.json"),
            output: Some(dir.join("review/review.html")),
            stdout: false,
            force: false,
            ai: true,
            ai_provider: Some(super::AiProviderArg::Openai),
            model: Some("gpt-test".to_string()),
            metadata_only: false,
            include_images: false,
            dry_run: false,
            ai_output: Some(dir.join("review/review.json")),
            markdown: Some(dir.join("review/review.md")),
            max_images: super::DEFAULT_AI_REVIEW_MAX_IMAGES,
            image_detail: AiImageDetailArg::High,
        },
        &mut openai_review,
    )
    .expect("review provider path writes artifacts");
    assert!(dir.join("review/review.json").exists());
    assert!(dir.join("review/review.md").exists());
    assert!(dir.join("review/review.html").exists());

    let mut openai_alt = |request: &AiAltRequest, project_root: &Path, key: &str| {
        assert_eq!(key, "unit-test-key");
        assert_eq!(project_root, dir.as_path());
        assert_eq!(request.model.as_deref(), Some("gpt-alt"));
        Ok(AiAltProviderPayload {
            summary: "alt ok".to_string(),
            drafts: Vec::new(),
        })
    };

    command_alt_with_openai(
        alt_args_with(&config, |args| {
            args.ai_provider = Some(super::AiProviderArg::Openai);
            args.model = Some("gpt-alt".to_string());
            args.include_images = true;
            args.output = Some(dir.join("alt/alt.json"));
            args.markdown = Some(dir.join("alt/alt.md"));
        }),
        &mut openai_alt,
    )
    .expect("alt provider path writes artifacts");
    assert!(dir.join("alt/alt.json").exists());
    assert!(dir.join("alt/alt.md").exists());

    let mut openai_draft = |request: &AiDraftRequest, key: &str| {
        assert_eq!(key, "unit-test-key");
        assert_eq!(request.model.as_deref(), Some("gpt-draft"));
        Ok(AiDraftProviderPayload {
            summary: "draft ok".to_string(),
            sections: Vec::new(),
        })
    };

    command_draft_with_openai(
        draft_args_with(&config, |args| {
            args.ai_provider = Some(super::AiProviderArg::Openai);
            args.model = Some("gpt-draft".to_string());
            args.output = Some(dir.join("draft/draft.md"));
        }),
        &mut openai_draft,
    )
    .expect("draft provider path writes artifact");
    assert!(dir.join("draft/draft.md").exists());

    restore_env("OPENAI_API_KEY", previous_openai);
    cleanup(&dir);
}

#[test]
fn provider_commands_reject_unsupported_image_inputs_before_network() {
    let dir = temp_dir("cli_provider_unsupported_images");
    let config = dir.join("devimg.toml");
    write_minimal_config(&config);
    fs::create_dir_all(dir.join("public/images")).expect("manifest dir");
    let mut manifest = cli_manifest("devimg.toml");
    let mut output = manifest.outputs.remove(0);
    output.format = "avif".to_string();
    output.source_path = "assets/images/source.avif".to_string();
    output.output_path = "public/images/generated/source.card.64.avif".to_string();
    manifest.outputs = vec![output];
    fs::write(
        dir.join("public/images/devimg-manifest.json"),
        serde_json::to_string_pretty(&manifest).expect("manifest serializes"),
    )
    .expect("write avif manifest");

    let mut unreachable_review =
        |_: &AiReviewRequest, _: &Path, _: &str| -> Result<AiReviewProviderPayload, CliError> {
            unreachable!("dry run should not call provider")
        };
    assert_config_error(command_review_ai_with_openai(
        ReviewArgs {
            manifest: dir.join("public/images/devimg-manifest.json"),
            output: None,
            stdout: false,
            force: false,
            ai: true,
            ai_provider: Some(super::AiProviderArg::Openai),
            model: Some("gpt-test".to_string()),
            metadata_only: false,
            include_images: true,
            dry_run: true,
            ai_output: Some(dir.join("review.json")),
            markdown: None,
            max_images: super::DEFAULT_AI_REVIEW_MAX_IMAGES,
            image_detail: AiImageDetailArg::Low,
        },
        &mut unreachable_review,
    ));

    let mut unreachable_alt =
        |_: &AiAltRequest, _: &Path, _: &str| -> Result<AiAltProviderPayload, CliError> {
            unreachable!("dry run should not call provider")
        };
    assert_config_error(command_alt_with_openai(
        alt_args_with(&config, |args| {
            args.ai_provider = Some(super::AiProviderArg::Openai);
            args.model = Some("gpt-alt".to_string());
            args.include_images = true;
            args.dry_run = true;
            args.output = Some(dir.join("alt.json"));
        }),
        &mut unreachable_alt,
    ));

    cleanup(&dir);
}

#[test]
fn provider_key_lookup_covers_env_file_env_var_and_missing_paths() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let dir = temp_dir("cli_provider_key_lookup");
    let previous_openai = std::env::var("OPENAI_API_KEY").ok();
    let previous_anthropic = std::env::var("ANTHROPIC_API_KEY").ok();
    std::env::remove_var("OPENAI_API_KEY");
    std::env::set_var("ANTHROPIC_API_KEY", "from-env");

    assert_eq!(
        read_ai_provider_key(
            Path::new("devimg.toml"),
            AiProvider::Anthropic,
            std::slice::from_ref(&dir),
            "test command",
        )
        .expect("env var key reads"),
        "from-env"
    );

    std::env::remove_var("ANTHROPIC_API_KEY");
    std::env::set_var("OPENAI_API_KEY", "   ");
    fs::write(
        dir.join(".env"),
        "OPENAI_API_KEY=\nOPENAI_API_KEY=from-file\n",
    )
    .expect("write env file");
    assert_eq!(
        read_ai_provider_key(
            Path::new("devimg.toml"),
            AiProvider::Openai,
            std::slice::from_ref(&dir),
            "test command",
        )
        .expect("env file key reads"),
        "from-file"
    );
    fs::remove_file(dir.join(".env")).expect("remove env file");
    assert_config_error(read_ai_provider_key(
        Path::new("devimg.toml"),
        AiProvider::Openai,
        std::slice::from_ref(&dir),
        "test command",
    ));
    assert_io_error(read_env_file_value(&dir, "OPENAI_API_KEY"));

    restore_env("OPENAI_API_KEY", previous_openai);
    restore_env("ANTHROPIC_API_KEY", previous_anthropic);
    cleanup(&dir);
}

#[test]
fn command_error_branches_cover_core_errors_after_config_load() {
    let dir = temp_dir("cli_core_error_branches");
    let config = dir.join("devimg.toml");
    write_minimal_config(&config);

    assert_config_error(command_check(CheckArgs {
        config: config.clone(),
        fail_on_warning: false,
        no_report: false,
    }));
    assert!(matches!(
        command_doctor(DoctorArgs {
            config: config.clone(),
            json: true,
            export_output: Some(dir.join("export.json")),
            export_format: ManifestExportFormat::Json,
            strip_prefix: None,
            url_prefix: String::new(),
            typescript_helpers: false,
        }),
        Err(CliError::DoctorFailed { .. })
    ));
    assert_config_error(command_ai_consent(AiConsentArgs {
        config,
        ai_provider: super::AiProviderArg::Openai,
        model: "gpt-test".to_string(),
        metadata_only: false,
        include_images: false,
        dry_run: true,
        output: None,
        force: false,
    }));

    cleanup(&dir);
}

#[test]
fn command_suggest_and_review_preflight_error_branches_are_covered() {
    let dir = temp_dir("cli_suggest_review_preflight");
    let config = dir.join("devimg.toml");
    write_minimal_config(&config);
    fs::create_dir_all(dir.join("assets/images")).expect("source dir");
    command_suggest(SuggestArgs {
        config: config.clone(),
        metadata_only: true,
        check: false,
        fail_on_severity: None,
        output: Some(dir.join("suggestions.json")),
        markdown: Some(dir.join("suggestions.md")),
        force: false,
    })
    .expect("suggest produces diagnostics for missing manifest");

    let escaping_config = dir.join("escaping.toml");
    fs::write(
        &escaping_config,
        r#"[project]
root = "."
manifest = "../outside.json"
report = "devimg-report.md"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"
include = ["**/*.png"]

[[preset]]
name = "card"
widths = [64]
formats = ["webp"]
quality = 82
"#,
    )
    .expect("write escaping config");
    assert_config_error(command_suggest(SuggestArgs {
        config: escaping_config,
        metadata_only: true,
        check: false,
        fail_on_severity: None,
        output: Some(dir.join("escaping-suggestions.json")),
        markdown: None,
        force: false,
    }));

    let manifest = dir.join("manifest.json");
    write_manifest_fixture(&manifest, &config.to_string_lossy());
    let mut unreachable_review =
        |_: &AiReviewRequest, _: &Path, _: &str| -> Result<AiReviewProviderPayload, CliError> {
            unreachable!("preflight should fail before provider")
        };
    assert_config_error(command_review_ai_with_openai(
        ReviewArgs {
            manifest,
            output: None,
            stdout: false,
            force: false,
            ai: true,
            ai_provider: Some(super::AiProviderArg::Openai),
            model: Some("gpt-test".to_string()),
            metadata_only: false,
            include_images: false,
            dry_run: true,
            ai_output: Some(dir.join("same.json")),
            markdown: Some(dir.join("same.json")),
            max_images: super::DEFAULT_AI_REVIEW_MAX_IMAGES,
            image_detail: AiImageDetailArg::Low,
        },
        &mut unreachable_review,
    ));

    cleanup(&dir);
}

#[test]
fn wrapper_transport_error_paths_are_mapped_by_each_openai_payload_parser() {
    assert_config_error(call_openai_review_with_transport(
        &review_request(false),
        Path::new("."),
        "secret-key",
        &FakeTransport::err(OpenAiTransportError::Request),
    ));
    assert_config_error(call_openai_alt_with_transport(
        &alt_request(false),
        Path::new("."),
        "secret-key",
        &FakeTransport::err(OpenAiTransportError::Request),
    ));
    assert_config_error(call_openai_draft_with_transport(
        &draft_request(),
        "secret-key",
        &FakeTransport::err(OpenAiTransportError::Request),
    ));
}

#[test]
fn path_helpers_cover_absolute_manifest_relative_review_and_label_branches() {
    let dir = temp_dir("cli_path_helper_branches");
    let project = dir.join("project");
    fs::create_dir_all(project.join("assets/images")).expect("source dir");
    fs::create_dir_all(project.join("public/images")).expect("manifest dir");
    let config_path = project.join("devimg.toml");
    write_minimal_config(&config_path);

    let manifest = cli_manifest(&config_path.to_string_lossy());
    assert_eq!(
        review_project_root(
            &project.join("public/images/devimg-manifest.json"),
            &manifest
        ),
        project
    );

    let absolute_no_match = dir.join("elsewhere/manifest.json");
    fs::create_dir_all(absolute_no_match.parent().expect("manifest parent"))
        .expect("absolute manifest parent");
    assert_eq!(
        review_project_root(&absolute_no_match, &cli_manifest("")),
        absolute_no_match.parent().unwrap()
    );
    assert_eq!(
        review_project_root(Path::new("/"), &cli_manifest("")),
        std::env::current_dir().unwrap()
    );
    assert_eq!(
        review_project_root(
            &project.join("public/images/devimg-manifest.json"),
            &cli_manifest("plain-devimg.toml")
        ),
        std::env::current_dir().unwrap()
    );
    assert_eq!(
        review_project_root(Path::new("relative-manifest.json"), &cli_manifest("/")),
        std::env::current_dir().unwrap()
    );
    assert_eq!(
        review_asset_path_prefix(
            Path::new("relative/review.html"),
            &std::env::current_dir().unwrap()
        )
        .expect("relative output prefix"),
        ".."
    );
    assert_eq!(
        review_asset_path_prefix(&dir.join("review.html"), &dir.join("nested/project"))
            .expect("project under output parent"),
        "nested/project"
    );
    assert_eq!(normalize_lexical(Path::new("./a/../b")), PathBuf::from("b"));
    write_text_file(&dir.join("write-text-file.txt"), "ok").expect("write temp fixture");
    write_text_file(&dir.join("nested/write-text-file.txt"), "ok")
        .expect("write nested temp fixture");
    assert_eq!(AiImageDetailArg::Auto.label(), "auto");
    assert_eq!(AiImageDetailArg::Original.label(), "original");

    let absolute_manifest_config = Config {
        project: ProjectConfig {
            manifest: dir.join("absolute-manifest.json"),
            ..test_config(&dir).project
        },
        ..test_config(&dir)
    };
    assert_eq!(
        agent_task_manifest_path(&absolute_manifest_config),
        dir.join("absolute-manifest.json")
    );

    cleanup(&dir);
}

#[test]
fn manifest_export_check_wraps_non_not_found_read_errors() {
    let dir = temp_dir("cli_manifest_export_read_error");
    let manifest_path = dir.join("manifest.json");
    write_manifest_fixture(&manifest_path, "devimg.toml");
    let output_dir = dir.join("export-dir");
    fs::create_dir_all(&output_dir).expect("create output dir");
    assert_io_error(command_manifest_export(ManifestExportArgs {
        manifest: manifest_path,
        format: ManifestExportFormat::Json,
        output: Some(output_dir),
        strip_prefix: None,
        url_prefix: String::new(),
        check: true,
        typescript_helpers: false,
    }));
    cleanup(&dir);
}

#[test]
fn file_write_helper_reports_parent_and_write_errors() {
    let dir = temp_dir("cli_file_write_helper_errors");
    let blocking_file = dir.join("file-parent");
    fs::write(&blocking_file, "file").expect("write blocking file");
    assert_io_error(command_init(InitArgs {
        config: blocking_file.join("nested/devimg.toml"),
        profile: None,
        stdout: false,
        force: false,
    }));
    assert_io_error(write_text_file(
        &blocking_file.join("nested.txt"),
        "contents",
    ));
    let output_dir = dir.join("output-dir");
    fs::create_dir_all(&output_dir).expect("create output directory");
    assert_io_error(write_text_file(&output_dir, "contents"));
    cleanup(&dir);
}

#[test]
fn manifest_export_check_and_error_branches_are_stable() {
    let dir = temp_dir("cli_manifest_export_branches");
    let manifest_path = dir.join("manifest.json");
    write_manifest_fixture(&manifest_path, "devimg.toml");
    let output = dir.join("export.json");

    assert_config_error(command_manifest_export(ManifestExportArgs {
        manifest: manifest_path.clone(),
        format: ManifestExportFormat::Json,
        output: Some(output.clone()),
        strip_prefix: None,
        url_prefix: String::new(),
        check: false,
        typescript_helpers: true,
    }));
    assert_config_error(command_manifest_export(ManifestExportArgs {
        manifest: manifest_path.clone(),
        format: ManifestExportFormat::Json,
        output: None,
        strip_prefix: None,
        url_prefix: String::new(),
        check: true,
        typescript_helpers: false,
    }));
    assert!(matches!(
        command_manifest_export(ManifestExportArgs {
            manifest: manifest_path.clone(),
            format: ManifestExportFormat::Json,
            output: Some(output.clone()),
            strip_prefix: None,
            url_prefix: String::new(),
            check: true,
            typescript_helpers: false,
        }),
        Err(CliError::CheckFailed(_))
    ));
    fs::write(&output, "{}\n").expect("write stale export");
    assert!(matches!(
        command_manifest_export(ManifestExportArgs {
            manifest: manifest_path.clone(),
            format: ManifestExportFormat::Json,
            output: Some(output.clone()),
            strip_prefix: None,
            url_prefix: String::new(),
            check: true,
            typescript_helpers: false,
        }),
        Err(CliError::CheckFailed(_))
    ));
    command_manifest_export(ManifestExportArgs {
        manifest: manifest_path.clone(),
        format: ManifestExportFormat::Json,
        output: Some(output.clone()),
        strip_prefix: None,
        url_prefix: String::new(),
        check: false,
        typescript_helpers: false,
    })
    .expect("write current export");
    command_manifest_export(ManifestExportArgs {
        manifest: manifest_path,
        format: ManifestExportFormat::Json,
        output: Some(output),
        strip_prefix: None,
        url_prefix: String::new(),
        check: true,
        typescript_helpers: false,
    })
    .expect("current export passes check");

    cleanup(&dir);
}

#[test]
fn review_project_root_and_agent_helpers_cover_fallbacks() {
    let dir = temp_dir("cli_root_agent_helpers");
    let project = dir.join("project");
    fs::create_dir_all(project.join("public/images/generated")).expect("create project dirs");
    let config_path = project.join("devimg.toml");
    write_minimal_config(&config_path);
    let source = project.join("assets/images/source.png");
    fs::create_dir_all(source.parent().expect("source parent")).expect("source dirs");
    fs::write(&source, b"not needed").expect("source placeholder");
    let output = project.join("public/images/generated/source.card.64.webp");
    fs::write(&output, b"not needed").expect("output placeholder");

    let manifest = cli_manifest(&config_path.to_string_lossy());
    assert_eq!(
        review_project_root(
            &project.join("public/images/devimg-manifest.json"),
            &manifest
        ),
        project
    );

    let invalid_config_manifest = cli_manifest("missing/devimg.toml");
    assert_eq!(
        review_project_root(
            &project.join("public/images/devimg-manifest.json"),
            &invalid_config_manifest
        ),
        normalize_lexical(&std::env::current_dir().unwrap().join("missing"))
    );

    let inferred = cli_manifest("");
    assert_eq!(
        review_project_root(
            &project.join("public/images/devimg-manifest.json"),
            &inferred
        ),
        project
    );
    assert_eq!(
        review_project_root(Path::new("relative-manifest.json"), &inferred),
        std::env::current_dir().unwrap()
    );

    let config = test_config(&project);
    assert_eq!(
        agent_task_manifest_path(&config),
        project.join("public/images/devimg-manifest.json")
    );
    let missing_artifacts = agent_task_generated_artifacts(&config);
    assert!(missing_artifacts.generated_variants.is_empty());
    assert!(missing_artifacts.manifest_read_error.is_none());
    fs::write(
        project.join("public/images/devimg-manifest.json"),
        serde_json::to_string_pretty(&cli_manifest("devimg.toml")).expect("manifest serializes"),
    )
    .expect("write manifest");
    let present_artifacts = agent_task_generated_artifacts(&config);
    assert_eq!(present_artifacts.generated_variants.len(), 1);
    fs::write(project.join("public/images/devimg-manifest.json"), "{").expect("corrupt manifest");
    let corrupt_artifacts = agent_task_generated_artifacts(&config);
    assert!(corrupt_artifacts.manifest_read_error.is_some());

    assert!(is_protected_agent_instruction_path(Path::new("AGENTS.md")));
    assert!(is_protected_agent_instruction_path(Path::new(
        ".claude/commands/devimg.md"
    )));
    assert!(!is_protected_agent_instruction_path(Path::new(
        "ai_tasks/task.md"
    )));
    assert!(!is_protected_agent_instruction_path(Path::new("/")));
    assert_eq!(comma_or_none(&[]), "none");
    assert_eq!(
        comma_or_none(&["next".to_string(), "vite".to_string()]),
        "next, vite"
    );

    cleanup(&dir);
}

#[test]
fn render_helpers_cover_empty_and_non_empty_sections() {
    let mut out = String::new();
    push_diagnostics(&mut out, &[], "No issues.");
    assert!(out.contains("No issues."));
    push_diagnostics(
        &mut out,
        &[DoctorDiagnostic {
            code: "missing_manifest".to_string(),
            path: "manifest.json".to_string(),
            message: "missing".to_string(),
            hint: "run optimize".to_string(),
        }],
        "No issues.",
    );
    assert!(out.contains("missing_manifest"));

    let mut paths = String::new();
    push_named_paths(&mut paths, "Generated", &[], "None.");
    push_named_paths(
        &mut paths,
        "Generated",
        &["one.webp".to_string(), "two.webp".to_string()],
        "None.",
    );
    assert!(paths.contains("None."));
    assert!(paths.contains("two.webp"));

    let report = SuggestionReport {
        version: 1,
        config_path: "devimg.toml".to_string(),
        mode: "metadata-only".to_string(),
        summary: SuggestionSummary {
            suggestion_count: 2,
            error_count: 1,
            warning_count: 1,
            advisory_count: 0,
        },
        items: vec![
            suggestion_item("error", false),
            suggestion_item("warning", true),
            suggestion_item("advisory", false),
        ],
    };
    assert_eq!(
        blocking_suggestion_count(&report, SuggestFailSeverity::Error),
        1
    );
    assert_eq!(
        blocking_suggestion_count(&report, SuggestFailSeverity::Warning),
        1
    );
    assert_eq!(
        blocking_suggestion_count(&report, SuggestFailSeverity::Advisory),
        3
    );
    let read_only =
        render_suggest_terminal_summary(&report, true, SuggestFailSeverity::Warning, 1, &[]);
    assert!(read_only.contains("Output: no output written"));
    let written = render_suggest_terminal_summary(
        &report,
        false,
        SuggestFailSeverity::Warning,
        0,
        &[PathBuf::from("suggestions.json")],
    );
    assert!(written.contains("Output written"));

    let hinted = with_check_hint("report".to_string(), Path::new("custom.toml"));
    assert!(hinted.contains("devimg optimize --config custom.toml --allow-overwrite"));
    assert!(
        render_core_error(&devimg_core::DevimgError::image("bad.png", "bad")).contains("inspect")
    );
    assert!(
        render_core_error(&devimg_core::DevimgError::UnsafeOverwrite {
            path: PathBuf::from("out")
        })
        .contains("overwrite")
    );
    assert!(render_core_error(&devimg_core::DevimgError::config(
        "devimg.toml",
        "devimg agent task refuses to write task output to agent instruction paths"
    ))
    .contains("ai_tasks/devimg-agent-task.md"));
    assert!(render_core_error(&devimg_core::DevimgError::config(
        "devimg.toml",
        "devimg suggest currently requires --metadata-only"
    ))
    .contains("devimg suggest --metadata-only"));
    assert!(render_core_error(&devimg_core::DevimgError::config(
        "devimg.toml",
        "--fail-on-severity requires --check"
    ))
    .contains("--fail-on-severity warning"));
}

#[test]
fn command_agent_task_and_render_agent_task_cover_generated_artifact_sections() {
    let dir = temp_dir("cli_agent_task_render");
    let config_path = dir.join("devimg.toml");
    write_minimal_config(&config_path);
    let config = test_config(&dir);
    let report = DoctorReport {
        status: "pass".to_string(),
        config_path: "devimg.toml".to_string(),
        project_root: dir.to_string_lossy().to_string(),
        manifest_path: "public/images/devimg-manifest.json".to_string(),
        report_path: "devimg-report.md".to_string(),
        frameworks: vec!["next".to_string()],
        manifest_helpers: vec!["lib/devimg.generated.ts".to_string()],
        source_image_count: 1,
        planned_variant_count: 1,
        generated_variant_count: 1,
        source_bytes: 10,
        output_bytes: 20,
        budget: DoctorBudget {
            status: "pass".to_string(),
            max_total_bytes: Some(100),
            max_file_bytes: Some(50),
            total_output_bytes: 20,
            total_headroom_bytes: Some(80),
        },
        checks: vec![DoctorCheck {
            name: "manifest".to_string(),
            status: "pass".to_string(),
            message: "ok".to_string(),
        }],
        warnings: Vec::new(),
        acknowledged_warnings: Vec::new(),
        issues: Vec::new(),
        next_command: "devimg check".to_string(),
    };
    let rendered = render_agent_task(
        &AgentTaskArgs {
            agent: AgentTaskAgent::Codex,
            config: PathBuf::from("devimg.toml"),
            output: None,
            force: false,
        },
        &config,
        &report,
        &AgentTaskGeneratedArtifacts {
            generated_variants: vec!["public/images/generated/one.webp".to_string()],
            manifest_read_error: Some("bad manifest".to_string()),
        },
    );
    assert!(rendered.contains("bad manifest"));
    assert!(rendered.contains("Codex final response"));

    let mut empty_checks_report = report.clone();
    empty_checks_report.checks.clear();
    let empty_checks_rendered = render_agent_task(
        &AgentTaskArgs {
            agent: AgentTaskAgent::Generic,
            config: PathBuf::from("devimg.toml"),
            output: None,
            force: false,
        },
        &config,
        &empty_checks_report,
        &AgentTaskGeneratedArtifacts {
            generated_variants: Vec::new(),
            manifest_read_error: None,
        },
    );
    assert!(empty_checks_rendered.contains("No checks reported."));

    let output = dir.join("tasks/devimg-agent-task.md");
    command_agent_task(AgentTaskArgs {
        agent: AgentTaskAgent::Generic,
        config: config_path,
        output: Some(output.clone()),
        force: false,
    })
    .expect("agent task writes nested output");
    assert!(output.exists());

    cleanup(&dir);
}

#[test]
fn command_wrappers_cover_success_failure_and_stdout_edges() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let dir = temp_dir("cli_command_wrapper_edges");
    let config = dir.join("devimg.toml");
    write_tiny_config(&config, 10);
    let source = dir.join("assets/images/source.png");
    write_png(&source);

    command_init(InitArgs {
        config: dir.join("stdout.toml"),
        profile: None,
        stdout: true,
        force: false,
    })
    .expect("init stdout succeeds");
    let existing_config = dir.join("existing.toml");
    fs::write(&existing_config, "existing").expect("write existing config");
    assert_unsafe_overwrite(command_init(InitArgs {
        config: existing_config,
        profile: None,
        stdout: false,
        force: false,
    }));

    command_optimize(OptimizeArgs {
        config: config.clone(),
        dry_run: false,
        allow_overwrite: false,
    })
    .expect("optimize writes generated outputs");
    let optimize_error_config = dir.join("optimize-error-root/devimg.toml");
    write_minimal_config(&optimize_error_config);
    assert_config_error(command_optimize(OptimizeArgs {
        config: optimize_error_config,
        dry_run: true,
        allow_overwrite: false,
    }));

    command_check(CheckArgs {
        config: config.clone(),
        fail_on_warning: false,
        no_report: true,
    })
    .expect("check passes without strict warnings");
    assert!(matches!(
        command_check(CheckArgs {
            config: config.clone(),
            fail_on_warning: true,
            no_report: false,
        }),
        Err(CliError::CheckFailed(report)) if report.contains("Next:")
    ));

    command_doctor(DoctorArgs {
        config: config.clone(),
        json: true,
        export_output: None,
        export_format: ManifestExportFormat::Json,
        strip_prefix: None,
        url_prefix: String::new(),
        typescript_helpers: false,
    })
    .expect("doctor JSON pass prints");
    assert!(matches!(
        command_doctor(DoctorArgs {
            config: config.clone(),
            json: false,
            export_output: Some(dir.join("missing-helper.ts")),
            export_format: ManifestExportFormat::Typescript,
            strip_prefix: Some("public".to_string()),
            url_prefix: "/".to_string(),
            typescript_helpers: true,
        }),
        Err(CliError::DoctorFailed { report, json: false }) if report.contains("DevImg Doctor")
    ));

    let manifest = dir.join("public/images/devimg-manifest.json");
    command_compare(CompareArgs {
        base: manifest.clone(),
        head: manifest.clone(),
        json: true,
        top: 1,
    })
    .expect("compare JSON prints");
    command_compare(CompareArgs {
        base: manifest.clone(),
        head: manifest.clone(),
        json: false,
        top: 1,
    })
    .expect("compare text prints");
    command_inspect(InspectArgs {
        files: vec![source],
    })
    .expect("inspect prints image details");

    command_manifest_export(ManifestExportArgs {
        manifest: manifest.clone(),
        format: ManifestExportFormat::Typescript,
        output: None,
        strip_prefix: Some("public".to_string()),
        url_prefix: "/".to_string(),
        check: false,
        typescript_helpers: true,
    })
    .expect("manifest export TS stdout succeeds");

    let previous_openai = std::env::var("OPENAI_API_KEY").ok();
    std::env::set_var("OPENAI_API_KEY", "unit-test-key");
    command_ai_consent(AiConsentArgs {
        config: config.clone(),
        ai_provider: super::AiProviderArg::Openai,
        model: "gpt-test".to_string(),
        metadata_only: false,
        include_images: false,
        dry_run: false,
        output: None,
        force: false,
    })
    .expect("consent reads key and prints JSON");
    std::env::remove_var("OPENAI_API_KEY");
    assert_config_error(command_ai_consent(AiConsentArgs {
        config: config.clone(),
        ai_provider: super::AiProviderArg::Openai,
        model: "gpt-test".to_string(),
        metadata_only: false,
        include_images: false,
        dry_run: false,
        output: None,
        force: false,
    }));
    let consent_output = dir.join("consent.json");
    fs::write(&consent_output, "{}").expect("write consent output");
    assert_unsafe_overwrite(command_ai_consent(AiConsentArgs {
        config: config.clone(),
        ai_provider: super::AiProviderArg::Openai,
        model: "gpt-test".to_string(),
        metadata_only: false,
        include_images: false,
        dry_run: true,
        output: Some(consent_output),
        force: false,
    }));
    restore_env("OPENAI_API_KEY", previous_openai);

    cleanup(&dir);
}

#[test]
fn review_suggest_alt_draft_cover_remaining_command_edges() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let previous_openai = std::env::var("OPENAI_API_KEY").ok();
    std::env::remove_var("OPENAI_API_KEY");
    let dir = temp_dir("cli_review_suggest_alt_draft_edges");
    let config = dir.join("devimg.toml");
    write_tiny_config(&config, 82);
    write_png(&dir.join("assets/images/source.png"));
    command_optimize(OptimizeArgs {
        config: config.clone(),
        dry_run: false,
        allow_overwrite: false,
    })
    .expect("optimize fixture");
    let manifest = dir.join("public/images/devimg-manifest.json");

    command_review(review_args_with(|args| {
        args.manifest = manifest.clone();
        args.output = None;
        args.stdout = true;
    }))
    .expect("review stdout prints");
    let review_output = dir.join("review.html");
    fs::write(&review_output, "existing").expect("write review html");
    assert_unsafe_overwrite(command_review(review_args_with(|args| {
        args.manifest = manifest.clone();
        args.output = Some(review_output);
    })));

    command_review(review_args_with(|args| {
        args.manifest = manifest.clone();
        args.output = Some(dir.join("ai-review.html"));
        args.ai = true;
        args.ai_provider = Some(super::AiProviderArg::Openai);
        args.model = Some("gpt-test".to_string());
        args.dry_run = true;
        args.ai_output = Some(dir.join("ai-review.json"));
        args.markdown = Some(dir.join("ai-review.md"));
    }))
    .expect("AI review dry-run writes all outputs");
    command_review_ai(review_args_with(|args| {
        args.manifest = manifest.clone();
        args.output = None;
        args.ai = true;
        args.ai_provider = Some(super::AiProviderArg::Openai);
        args.model = Some("gpt-test".to_string());
        args.dry_run = true;
        args.ai_output = None;
        args.markdown = None;
    }))
    .expect("AI review dry-run supports JSON-only output");
    let existing_ai = dir.join("existing-ai-review.json");
    fs::write(&existing_ai, "{}").expect("write AI output");
    assert_unsafe_overwrite(command_review_ai(review_args_with(|args| {
        args.manifest = manifest.clone();
        args.ai = true;
        args.ai_provider = Some(super::AiProviderArg::Openai);
        args.model = Some("gpt-test".to_string());
        args.dry_run = true;
        args.ai_output = Some(existing_ai);
    })));
    assert_config_error(command_review_ai(review_args_with(|args| {
        args.manifest = manifest.clone();
        args.ai = true;
        args.ai_provider = Some(super::AiProviderArg::Openai);
        args.model = Some("gpt-test".to_string());
        args.dry_run = false;
        args.ai_output = Some(dir.join("missing-key-review.json"));
    })));

    assert_config_error(command_suggest(SuggestArgs {
        config: config.clone(),
        metadata_only: false,
        check: false,
        fail_on_severity: None,
        output: None,
        markdown: None,
        force: false,
    }));
    assert_config_error(command_suggest(SuggestArgs {
        config: config.clone(),
        metadata_only: true,
        check: false,
        fail_on_severity: Some(SuggestFailSeverity::Warning),
        output: None,
        markdown: None,
        force: false,
    }));
    assert_config_error(command_suggest(SuggestArgs {
        config: config.clone(),
        metadata_only: true,
        check: false,
        fail_on_severity: None,
        output: Some(dir.join("same-suggestions.md")),
        markdown: Some(dir.join("same-suggestions.md")),
        force: false,
    }));
    let existing_suggestions = dir.join("suggestions.json");
    fs::write(&existing_suggestions, "{}").expect("write suggestions");
    assert_unsafe_overwrite(command_suggest(SuggestArgs {
        config: config.clone(),
        metadata_only: true,
        check: false,
        fail_on_severity: None,
        output: Some(existing_suggestions),
        markdown: None,
        force: false,
    }));
    let missing_manifest_root = dir.join("missing-manifest-project");
    let missing_manifest_config = missing_manifest_root.join("devimg.toml");
    write_tiny_config(&missing_manifest_config, 82);
    assert!(matches!(
        command_suggest(SuggestArgs {
            config: missing_manifest_config,
            metadata_only: true,
            check: true,
            fail_on_severity: Some(SuggestFailSeverity::Advisory),
            output: Some(missing_manifest_root.join("check-suggestions.json")),
            markdown: None,
            force: false,
        }),
        Err(CliError::SuggestCheckFailed(summary)) if summary.contains("Suggestions")
    ));
    command_suggest(SuggestArgs {
        config: config.clone(),
        metadata_only: true,
        check: true,
        fail_on_severity: None,
        output: None,
        markdown: None,
        force: false,
    })
    .expect("suggest check can be read-only");
    command_suggest(SuggestArgs {
        config: config.clone(),
        metadata_only: true,
        check: false,
        fail_on_severity: None,
        output: None,
        markdown: None,
        force: false,
    })
    .expect("suggest writes default JSON output");

    command_alt(alt_args_with(&config, |args| {
        args.output = None;
        args.markdown = Some(dir.join("alt.md"));
    }))
    .expect("alt placeholder writes default JSON and markdown");
    command_alt(alt_args_with(&config, |args| {
        args.output = Some(dir.join("alt-no-markdown.json"));
        args.markdown = None;
    }))
    .expect("alt placeholder can skip markdown");
    let existing_alt = dir.join("existing-alt.json");
    fs::write(&existing_alt, "{}").expect("write alt");
    assert_unsafe_overwrite(command_alt(alt_args_with(&config, |args| {
        args.output = Some(existing_alt);
    })));
    assert_config_error(command_alt(alt_args_with(&config, |args| {
        args.ai_provider = Some(super::AiProviderArg::Openai);
        args.model = Some("gpt-alt".to_string());
        args.include_images = true;
        args.dry_run = false;
        args.output = Some(dir.join("missing-key-alt.json"));
    })));

    command_draft(draft_args_with(&config, |args| {
        args.output = Some(dir.join("draft.md"));
    }))
    .expect("draft placeholder writes markdown");
    command_draft(draft_args_with(&config, |_| {}))
        .expect("draft placeholder writes default markdown");
    let existing_draft = dir.join("existing-draft.md");
    fs::write(&existing_draft, "existing").expect("write draft");
    assert_unsafe_overwrite(command_draft(draft_args_with(&config, |args| {
        args.output = Some(existing_draft);
    })));
    assert_config_error(command_draft(draft_args_with(&config, |args| {
        args.ai_provider = Some(super::AiProviderArg::Anthropic);
        args.model = Some("claude-test".to_string());
        args.dry_run = false;
        args.output = Some(dir.join("anthropic-draft.md"));
    })));
    assert_config_error(command_draft(draft_args_with(&config, |args| {
        args.ai_provider = Some(super::AiProviderArg::Openai);
        args.model = Some("gpt-draft".to_string());
        args.dry_run = false;
        args.output = Some(dir.join("missing-key-draft.md"));
    })));

    restore_env("OPENAI_API_KEY", previous_openai);
    cleanup(&dir);
}

#[test]
fn agent_commands_render_errors_stdout_and_variant_labels() {
    let dir = temp_dir("cli_agent_edges");
    let config = dir.join("devimg.toml");
    write_minimal_config(&config);

    command_agent(AgentArgs {
        command: AgentCommand::Task(AgentTaskArgs {
            agent: AgentTaskAgent::ClaudeCode,
            config: config.clone(),
            output: None,
            force: false,
        }),
    })
    .expect("agent task wrapper stdout succeeds");
    assert_config_error(command_agent_task(AgentTaskArgs {
        agent: AgentTaskAgent::Codex,
        config: config.clone(),
        output: Some(PathBuf::from("AGENTS.md")),
        force: false,
    }));
    let existing_task = dir.join("task.md");
    fs::write(&existing_task, "existing").expect("write existing task");
    assert_unsafe_overwrite(command_agent_task(AgentTaskArgs {
        agent: AgentTaskAgent::Generic,
        config: config.clone(),
        output: Some(existing_task),
        force: false,
    }));

    command_agent_init(AgentInitArgs {
        target: AgentTarget::Both,
        output_dir: dir.join("stdout-agent"),
        config: PathBuf::from("devimg.toml"),
        stdout: true,
        force: false,
    })
    .expect("agent init stdout prints both files");
    let agent_dir = dir.join("agent");
    command_agent_init(AgentInitArgs {
        target: AgentTarget::Codex,
        output_dir: agent_dir.clone(),
        config: PathBuf::from("devimg.toml"),
        stdout: false,
        force: false,
    })
    .expect("agent init writes file");
    assert_unsafe_overwrite(command_agent_init(AgentInitArgs {
        target: AgentTarget::Codex,
        output_dir: agent_dir,
        config: PathBuf::from("devimg.toml"),
        stdout: false,
        force: false,
    }));
    command_agent_init(AgentInitArgs {
        target: AgentTarget::Codex,
        output_dir: dir.join("agent"),
        config: PathBuf::from("devimg.toml"),
        stdout: false,
        force: true,
    })
    .expect("agent init force overwrites existing instruction file");

    assert_eq!(SuggestFailSeverity::Advisory.label(), "advisory");
    assert_eq!(SuggestFailSeverity::Error.label(), "error");
    assert_eq!(
        DraftTypeArg::ReadmeSnippet.into_core(),
        AiDraftType::ReadmeSnippet
    );
    assert_eq!(
        DraftTypeArg::ProjectPageCopy.into_core(),
        AiDraftType::ProjectPageCopy
    );
    assert_eq!(
        DraftTypeArg::BlogOutline.into_core(),
        AiDraftType::BlogOutline
    );
    assert_eq!(
        DraftTypeArg::SocialPostOutline.into_core(),
        AiDraftType::SocialPostOutline
    );
    assert_eq!(AgentTaskAgent::ClaudeCode.label(), "claude-code");
    assert!(AgentTaskAgent::ClaudeCode
        .final_response_guidance()
        .contains("Claude Code"));
    assert!(starter_config(None).contains("assets/images"));
    assert!(starter_config(Some(InitProfile::Next)).contains("public/images/source"));
    assert!(starter_config(Some(InitProfile::Astro)).contains("src/assets/images"));

    cleanup(&dir);
}

#[test]
fn render_core_error_and_error_variants_cover_remaining_branches() {
    assert!(matches!(
        current_dir_error(std::io::Error::other("cwd")),
        devimg_core::DevimgError::Io { .. }
    ));
    assert!(render_core_error(&devimg_core::DevimgError::config(
        "devimg.toml",
        "config file not found"
    ))
    .contains("devimg init"));
    assert!(render_core_error(&devimg_core::DevimgError::config(
        "devimg.toml",
        "bad field"
    ))
    .contains("devimg doctor"));
    assert!(render_core_error(&devimg_core::DevimgError::io(
        "devimg.toml",
        std::io::Error::other("nope")
    ))
    .starts_with("Error:"));
    assert!(
        render_core_error(&devimg_core::DevimgError::CheckFailed { issues: Vec::new() })
            .starts_with("Error:")
    );

    let parse_error = clap::Command::new("devimg-test")
        .try_get_matches_from(["devimg-test", "--bad"])
        .expect_err("parse error");
    match CliError::Parse(parse_error) {
        CliError::Parse(error) => assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument),
        other => panic!("expected parse error, got {other:?}"),
    }
    assert!(matches!(
        CliError::DoctorFailed {
            report: "report".to_string(),
            json: true
        },
        CliError::DoctorFailed { report, json: true } if report == "report"
    ));
}

fn review_args() -> ReviewArgs {
    ReviewArgs {
        manifest: PathBuf::from("manifest.json"),
        output: Some(PathBuf::from("review.html")),
        stdout: false,
        force: false,
        ai: false,
        ai_provider: None,
        model: None,
        metadata_only: false,
        include_images: false,
        dry_run: false,
        ai_output: None,
        markdown: None,
        max_images: super::DEFAULT_AI_REVIEW_MAX_IMAGES,
        image_detail: AiImageDetailArg::Low,
    }
}

fn review_args_with(update: impl FnOnce(&mut ReviewArgs)) -> ReviewArgs {
    let mut args = review_args();
    update(&mut args);
    args
}

fn alt_args_with(config: &Path, update: impl FnOnce(&mut AltArgs)) -> AltArgs {
    let mut args = AltArgs {
        config: config.to_path_buf(),
        ai_provider: None,
        model: None,
        metadata_only: false,
        include_images: false,
        dry_run: false,
        output: None,
        markdown: None,
        force: false,
        max_images: super::DEFAULT_AI_REVIEW_MAX_IMAGES,
        image_detail: AiImageDetailArg::Low,
    };
    update(&mut args);
    args
}

fn draft_args_with(config: &Path, update: impl FnOnce(&mut DraftArgs)) -> DraftArgs {
    let mut args = DraftArgs {
        config: config.to_path_buf(),
        draft_type: DraftTypeArg::ReleaseNotes,
        metadata_only: false,
        ai_provider: None,
        model: None,
        dry_run: false,
        output: None,
        force: false,
        compare_json: None,
        ai_review_json: None,
        review_html: None,
        changelog: None,
    };
    update(&mut args);
    args
}

fn write_minimal_config(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create config parent");
    }
    fs::write(
        path,
        r#"[project]
root = "."
manifest = "public/images/devimg-manifest.json"
report = "devimg-report.md"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"
include = ["**/*.png"]

[[preset]]
name = "card"
widths = [64]
formats = ["webp"]
quality = 82
fit = "cover"
aspect_ratio = "1:1"
"#,
    )
    .expect("write config fixture");
}

fn write_tiny_config(path: &Path, quality: u8) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create config parent");
    }
    fs::write(
        path,
        format!(
            r#"[project]
root = "."
manifest = "public/images/devimg-manifest.json"
report = "devimg-report.md"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"
include = ["**/*.png"]

[[preset]]
name = "card"
widths = [1]
formats = ["webp"]
quality = {quality}
fit = "contain"
"#
        ),
    )
    .expect("write tiny config fixture");
}

fn write_png(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create png parent");
    }
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/images/sample.png"),
        path,
    )
    .expect("copy png fixture");
}

fn write_manifest_fixture(path: &Path, config_path: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create manifest parent");
    }
    fs::write(
        path,
        serde_json::to_string_pretty(&cli_manifest(config_path)).expect("manifest serializes"),
    )
    .expect("write manifest fixture");
}

fn cli_manifest(config_path: &str) -> Manifest {
    Manifest {
        version: 1,
        generated_at: "unix:123".to_string(),
        config_path: config_path.to_string(),
        config_hash: "blake3:config".to_string(),
        outputs: vec![ManifestOutput {
            source_path: "assets/images/source.png".to_string(),
            source_hash: "blake3:source".to_string(),
            source_width: 64,
            source_height: 64,
            source_bytes: 10,
            output_path: "public/images/generated/source.card.64.webp".to_string(),
            preset: "card".to_string(),
            fit: "cover".to_string(),
            width: 64,
            height: 64,
            format: "webp".to_string(),
            bytes: 20,
            hash: "blake3:output".to_string(),
            operation_hash: "blake3:operation".to_string(),
        }],
    }
}

fn test_config(root: &Path) -> Config {
    Config {
        path: root.join("devimg.toml"),
        config_hash: "blake3:config".to_string(),
        project: ProjectConfig {
            root: root.to_path_buf(),
            manifest: PathBuf::from("public/images/devimg-manifest.json"),
            report: PathBuf::from("devimg-report.md"),
            overwrite: false,
            strip_metadata: true,
            content_hash_filenames: false,
        },
        sources: vec![SourceConfig {
            name: "portfolio".to_string(),
            input: PathBuf::from("assets/images"),
            output: PathBuf::from("public/images/generated"),
            include: vec!["**/*.png".to_string()],
            exclude: Vec::new(),
        }],
        presets: Vec::new(),
        overrides: Vec::new(),
        warnings: devimg_core::WarningConfig::default(),
        budgets: devimg_core::BudgetConfig::default(),
    }
}

fn suggestion_item(severity: &str, acknowledged: bool) -> SuggestionItem {
    SuggestionItem {
        id: format!("{severity}:{acknowledged}"),
        source_path: Some("assets/images/source.png".to_string()),
        source_kind: Some("source".to_string()),
        output_path: Some("public/images/generated/source.webp".to_string()),
        affected_path: "public/images/generated/source.webp".to_string(),
        preset: Some("card".to_string()),
        width: Some(64),
        format: Some("webp".to_string()),
        warning_code: "quality:cover-crop".to_string(),
        severity: severity.to_string(),
        acknowledged,
        rationale: "review".to_string(),
        suggested_config: None,
        commands: Vec::new(),
        next_command: String::new(),
    }
}

fn review_request(include_images: bool) -> AiReviewRequest {
    AiReviewRequest {
        schema_version: 1,
        provider: AiProvider::Openai,
        model: "gpt-test".to_string(),
        command: "devimg review --ai".to_string(),
        manifest_path: "devimg-manifest.json".to_string(),
        project_root: ".".to_string(),
        mode: if include_images {
            "image-backed".to_string()
        } else {
            "metadata-only".to_string()
        },
        dry_run: false,
        paths_included: true,
        image_bytes_included: include_images,
        image_detail: "original".to_string(),
        max_images: 8,
        ai_output_path: Some("review.json".to_string()),
        markdown_path: Some("review.md".to_string()),
        manifest_summary: AiReviewManifestSummary {
            config_path: "devimg.toml".to_string(),
            config_hash: "hash".to_string(),
            source_count: 1,
            output_count: 1,
            source_bytes: 10,
            output_bytes: 20,
        },
        review_signals: vec!["quality:cover-crop".to_string()],
        outputs: vec![AiReviewOutput {
            source_path: "assets/photo.png".to_string(),
            output_path: "generated/review.webp".to_string(),
            preset: "card".to_string(),
            fit: "cover".to_string(),
            width: 64,
            height: 64,
            format: "webp".to_string(),
            bytes: 20,
            hash: "hash".to_string(),
            source_width: 64,
            source_height: 64,
            source_bytes: 10,
            image_bytes_included: include_images,
            image_detail: include_images.then(|| "original".to_string()),
        }],
        selected_images: if include_images {
            vec![AiReviewImageInput {
                output_path: "generated/review.webp".to_string(),
                mime_type: "image/webp".to_string(),
                width: 64,
                height: 64,
                bytes: 20,
                hash: "hash".to_string(),
                detail: "original".to_string(),
            }]
        } else {
            Vec::new()
        },
        skipped_image_count: 0,
    }
}

fn alt_request(include_images: bool) -> AiAltRequest {
    AiAltRequest {
        schema_version: 1,
        provider: Some(AiProvider::Openai),
        model: Some("gpt-alt".to_string()),
        command: "devimg alt".to_string(),
        config_path: "devimg.toml".to_string(),
        manifest_path: "devimg-manifest.json".to_string(),
        project_root: ".".to_string(),
        mode: if include_images {
            "image-backed".to_string()
        } else {
            "metadata-only".to_string()
        },
        dry_run: false,
        paths_included: true,
        image_bytes_included: include_images,
        image_detail: "auto".to_string(),
        max_images: 8,
        output_path: Some("alt.json".to_string()),
        markdown_path: Some("alt.md".to_string()),
        manifest_summary: AiAltManifestSummary {
            config_path: "devimg.toml".to_string(),
            config_hash: "hash".to_string(),
            source_count: 1,
            output_count: 1,
            source_bytes: 10,
            output_bytes: 20,
        },
        sources: vec![AiAltSource {
            source_path: "assets/photo.png".to_string(),
            source_hash: "source-hash".to_string(),
            source_width: 64,
            source_height: 64,
            source_bytes: 10,
            representative_image_path: "generated/alt.png".to_string(),
            representative_image_width: 64,
            representative_image_height: 64,
            representative_image_format: "png".to_string(),
            image_bytes_included: include_images,
            image_detail: include_images.then(|| "auto".to_string()),
            variants: vec![AiAltVariant {
                output_path: "generated/alt.png".to_string(),
                preset: "card".to_string(),
                fit: "cover".to_string(),
                width: 64,
                height: 64,
                format: "png".to_string(),
                bytes: 20,
                hash: "hash".to_string(),
            }],
        }],
        selected_images: if include_images {
            vec![AiAltImageInput {
                source_path: "assets/photo.png".to_string(),
                image_path: "generated/alt.png".to_string(),
                mime_type: "image/png".to_string(),
                width: 64,
                height: 64,
                bytes: 20,
                hash: "hash".to_string(),
                detail: "auto".to_string(),
            }]
        } else {
            Vec::new()
        },
        skipped_image_count: 0,
    }
}

fn draft_request() -> AiDraftRequest {
    let options = AiDraftOptions {
        draft_type: AiDraftType::ReleaseNotes,
        provider: Some(AiProvider::Openai),
        model: Some("gpt-draft".to_string()),
        command: "devimg draft".to_string(),
        config_path: PathBuf::from("devimg.toml"),
        project_root: PathBuf::from("."),
        output_path: PathBuf::from("draft.md"),
        dry_run: false,
        compare_json_path: None,
        ai_review_json_path: None,
        review_html_path: None,
        changelog_path: None,
    };
    AiDraftRequest {
        schema_version: 1,
        draft_type: options.draft_type,
        provider: options.provider,
        model: options.model,
        command: options.command,
        config_path: "devimg.toml".to_string(),
        project_root: ".".to_string(),
        mode: "provider".to_string(),
        dry_run: false,
        output_path: "draft.md".to_string(),
        manifest_summary: devimg_core::AiDraftManifestSummary {
            path: "public/images/devimg-manifest.json".to_string(),
            readable: false,
            read_error: Some("missing".to_string()),
            config_hash: None,
            source_count: 0,
            output_count: 0,
            source_bytes: 0,
            output_bytes: 0,
            outputs: Vec::new(),
        },
        report_summary: devimg_core::AiDraftArtifactSummary {
            label: "report".to_string(),
            path: "devimg-report.md".to_string(),
            readable: false,
            read_error: Some("missing".to_string()),
            bytes: 0,
            line_count: 0,
            excerpt: String::new(),
        },
        changelog_summary: None,
        optional_artifacts: Vec::new(),
    }
}

fn output_text(text: &str) -> Value {
    json!({
        "output": [
            {
                "type": "message",
                "content": [
                    {
                        "type": "output_text",
                        "text": text
                    }
                ]
            }
        ]
    })
}

struct FakeTransport {
    response: std::result::Result<Value, OpenAiTransportError>,
    seen: RefCell<Vec<(String, Value)>>,
}

impl FakeTransport {
    fn ok(value: Value) -> Self {
        Self {
            response: Ok(value),
            seen: RefCell::new(Vec::new()),
        }
    }

    fn err(error: OpenAiTransportError) -> Self {
        Self {
            response: Err(error),
            seen: RefCell::new(Vec::new()),
        }
    }
}

impl OpenAiResponsesTransport for FakeTransport {
    fn post_responses(
        &self,
        api_key: &str,
        body: &Value,
    ) -> std::result::Result<Value, OpenAiTransportError> {
        self.seen
            .borrow_mut()
            .push((api_key.to_string(), body.clone()));
        self.response.clone()
    }
}

fn assert_config_error<T: std::fmt::Debug>(result: Result<T, CliError>) {
    match result {
        Err(CliError::Core(devimg_core::DevimgError::Config { .. })) => {}
        other => panic!("expected config error, got {other:?}"),
    }
}

fn assert_io_error<T: std::fmt::Debug>(result: Result<T, CliError>) {
    match result {
        Err(CliError::Core(devimg_core::DevimgError::Io { .. })) => {}
        other => panic!("expected I/O error, got {other:?}"),
    }
}

fn assert_unsafe_overwrite<T: std::fmt::Debug>(result: Result<T, CliError>) {
    match result {
        Err(CliError::Core(devimg_core::DevimgError::UnsafeOverwrite { .. })) => {}
        other => panic!("expected unsafe overwrite error, got {other:?}"),
    }
}

fn restore_env(name: &str, previous: Option<String>) {
    if let Some(value) = previous {
        std::env::set_var(name, value);
    } else {
        std::env::remove_var(name);
    }
}

fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("devimg-{label}-{nanos}"));
    fs::create_dir_all(&path).expect("create temp dir");
    path
}

fn cleanup(path: &Path) {
    let _ = fs::remove_dir_all(path);
}
