//! The fixture of the suite `module_v1` for the settings module: declared forms, commands and the valid calls.
//!
//! Pattern: Template Method (the rules are the suite's; this file is the fixture). Level 1.
#![allow(dead_code)]

use fs_interfaces::formsource_capnp::form_source;
use fs_module::{
    CommandKind, DeclaredCommand, DeclaredForm, FormSourceFixture, Param, Sample, Verify,
};

use super::{FORM, WEEK};

/// The suite's view of the settings module: one connection of a person.
pub struct SettingsFixture {
    pub client: form_source::Client,
}

fn param(name: &str, required: bool) -> Param {
    Param {
        name: name.into(),
        required,
    }
}

impl FormSourceFixture for SettingsFixture {
    fn client(&self) -> form_source::Client {
        self.client.clone()
    }

    fn level(&self) -> u32 {
        fs_settings_rpc::MODULE_LEVEL
    }

    fn provider(&self) -> Option<&str> {
        Some("settings")
    }

    fn forms(&self) -> Vec<DeclaredForm> {
        let fields = [
            "label",
            "basic",
            "value",
            "group",
            "hint",
            "default",
            "choices",
            "min",
            "max",
            "layer",
            "locked",
            "lock-reason",
            "order",
        ];
        vec![DeclaredForm {
            key: FORM.into(),
            scope: String::new(),
            fields: fields.map(String::from).to_vec(),
        }]
    }

    fn commands(&self) -> Vec<DeclaredCommand> {
        // The record IS the key (the rules decide by the record); `key` and `program` may only repeat what is known.
        let optional = [param("key", false), param("program", false)];
        let set = [vec![param("value", true)], optional.to_vec()].concat();
        vec![
            DeclaredCommand {
                key: "value.set".into(),
                kind: CommandKind::Record,
                params: set,
            },
            DeclaredCommand {
                key: "value.reset".into(),
                kind: CommandKind::Record,
                params: optional.to_vec(),
            },
        ]
    }

    fn samples(&self) -> Vec<Sample> {
        let value = |text: &str| Verify::Value {
            form: FORM.into(),
            record: Some(WEEK.into()),
            path: "value".into(),
            text: text.into(),
        };
        let call = |command: &str, args: Vec<(String, String)>, text: &str| Sample {
            command: command.into(),
            record: WEEK.into(),
            args,
            makes_record: true,
            verify: value(text),
        };
        vec![
            call("value.set", vec![("value".into(), "3".into())], "3"),
            call("value.reset", vec![], "1"),
        ]
    }

    fn strict_scope(&self) -> bool {
        true
    }
}
