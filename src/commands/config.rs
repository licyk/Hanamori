use crate::config::ResolvedConfig;
use anyhow::Result;

pub(super) fn show(config: &ResolvedConfig, resolved: bool, json: bool) -> Result<()> {
    let mut value = config.to_json();
    if !json {
        let settings = value.as_object().unwrap();
        if resolved {
            let rows = settings
                .iter()
                .map(|(name, setting)| {
                    let origin = match setting.get("derived_from") {
                        Some(derived) => format!(
                            "{} ({}: {})",
                            setting["source"].as_str().unwrap(),
                            derived["field"].as_str().unwrap(),
                            derived["source"].as_str().unwrap()
                        ),
                        None => setting["source"].as_str().unwrap().to_owned(),
                    };
                    [name.clone(), display_value(&setting["value"]), origin]
                })
                .collect();
            print!(
                "{}",
                super::output::table(["Setting", "Value", "Source"], rows)
            );
        } else {
            let rows = settings
                .iter()
                .map(|(name, setting)| [name.clone(), display_value(&setting["value"])])
                .collect();
            print!("{}", super::output::table(["Setting", "Value"], rows));
        }
        return Ok(());
    }
    if !resolved {
        for setting in value.as_object_mut().unwrap().values_mut() {
            *setting = setting["value"].take();
        }
    }
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn display_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Null => "none".to_owned(),
        value => value.to_string(),
    }
}
