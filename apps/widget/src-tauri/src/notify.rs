use serde::Serialize;

/// What the user did with a survey notification, sent to the webview as a `toast-answer` event.
/// `folder` is set when a folder button was pressed; `later` for "Позже"; neither when the toast itself
/// was clicked, which opens the full survey in the widget.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub struct ToastAnswer {
    pub id: String,
    pub folder: Option<String>,
    pub later: bool,
}

/// Windows allows five buttons on a toast: up to four folders plus "Позже".
#[cfg_attr(not(windows), allow(dead_code))]
pub const MAX_TOAST_FOLDERS: usize = 4;

/// Maps the clicked button's argument back to an answer. Arguments are indices, never folder names,
/// so nothing the user typed ends up in the toast XML attributes.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn answer_for(id: &str, folders: &[String], action: Option<&str>) -> ToastAnswer {
    let mut answer = ToastAnswer {
        id: id.to_string(),
        folder: None,
        later: false,
    };
    match action {
        Some("later") => answer.later = true,
        Some(index) => {
            answer.folder = index
                .parse::<usize>()
                .ok()
                .and_then(|i| folders.get(i).cloned())
        }
        None => {}
    }
    answer
}

/// The crate escapes text lines but not button labels, which sit in a single-quoted XML attribute.
#[cfg_attr(not(windows), allow(dead_code))]
fn escape_attr(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "&apos;")
        .replace('"', "&quot;")
}

/// Shows a Windows notification with the capture and its suggested folders as buttons ("answer the survey
/// from the notification"). Returns `false` where such notifications are not available (macOS, Linux,
/// mobile), so the webview simply waits for the usual survey.
#[tauri::command]
pub fn show_survey_toast(
    app: tauri::AppHandle,
    id: String,
    title: String,
    body: String,
    folders: Vec<String>,
) -> Result<bool, String> {
    show(app, id, title, body, folders)
}

#[cfg(windows)]
fn show(
    app: tauri::AppHandle,
    id: String,
    title: String,
    body: String,
    mut folders: Vec<String>,
) -> Result<bool, String> {
    use tauri::{Emitter, Manager};
    use tauri_winrt_notification::{Duration, Toast};

    folders.truncate(MAX_TOAST_FOLDERS);
    // Installed builds are registered under the bundle identifier by the installer; dev builds are not,
    // and borrow PowerShell's id the way tauri-plugin-notification does.
    let app_id = if tauri::is_dev() {
        Toast::POWERSHELL_APP_ID.to_string()
    } else {
        app.config().identifier.clone()
    };
    let mut toast = Toast::new(&app_id)
        .title(&format!("Куда положить «{title}»?"))
        .text1(&body)
        .text2("Кнопка раскладывает сразу. Нажми на уведомление, чтобы ответить на весь опрос.")
        .duration(Duration::Long);
    for (index, folder) in folders.iter().enumerate() {
        toast = toast.add_button(&escape_attr(folder), &index.to_string());
    }
    toast = toast.add_button("Позже", "later");
    let handle = app.clone();
    toast
        .on_activated(move |action| {
            let answer = answer_for(&id, &folders, action.as_deref());
            if answer.folder.is_none() && !answer.later {
                crate::desktop::summon(&handle, false);
            }
            if let Some(window) = handle.get_webview_window("main") {
                let _ = window.emit("toast-answer", answer);
            }
            Ok(())
        })
        .show()
        .map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(not(windows))]
fn show(
    _app: tauri::AppHandle,
    _id: String,
    _title: String,
    _body: String,
    _folders: Vec<String>,
) -> Result<bool, String> {
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_buttons_to_answers() {
        let folders = vec!["Учеба".to_string(), "Работа/Идеи".to_string()];
        assert_eq!(
            answer_for("x", &folders, Some("1")).folder.as_deref(),
            Some("Работа/Идеи")
        );
        assert!(answer_for("x", &folders, Some("later")).later);
        let opened = answer_for("x", &folders, None);
        assert!(opened.folder.is_none() && !opened.later);
        assert!(answer_for("x", &folders, Some("7")).folder.is_none());
    }

    #[test]
    fn escapes_button_labels() {
        assert_eq!(escape_attr("A'&<b>\""), "A&apos;&amp;&lt;b&gt;&quot;");
    }
}
