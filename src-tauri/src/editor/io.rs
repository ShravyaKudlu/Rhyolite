//! This module provides IO related functions for the app.
use std::fs; //Filesystem module
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager, State, Window};
use uuid::Uuid; 
// use tauri_plugin_dialog::DialogExt; //DialogExt trait to show dialog boxes

use dirs; 
use sanitize_filename; 

use crate::app_state::{AppState, CommandRegistrar, CommandRegistry, DocumentData, Tab, UserData};
use crate::editor::markdown_handler;
use crate::editor::tabs::update_tabs_state;


use crate::FileInfo; 

pub struct IOCommands;

impl IOCommands {
    ///BUG: The save_document command does not save the document
    ///when called from the frontend using exec_command.
    pub fn save_document(app: AppHandle, payload: Option<String>) {
        let Some(payload) = payload else {
            log::warn!("Invalid call to save_document");
            return;
        };
        log::debug!("save_document init");
        let temp_app = app.clone();
        let state = &temp_app.state::<AppState>();

        if let Ok(document_data) = serde_json::from_str::<DocumentData>(&payload) {
            {
                let mut workspace = state.workspace.write().unwrap();
                if let Some(doc) = workspace
                    .recent_files
                    .iter_mut()
                    .find(|doc| doc.id == document_data.id)
                {
                    doc.title = document_data.title.clone();
                } else {
                    workspace.recent_files.push(FileInfo {
                        id: document_data.id.clone(),
                        title: document_data.title.clone(),
                    });
                }
            }

            let trove_dir = get_trove_dir("Untitled_Trove");
            let markdown_content = markdown_handler::html_to_markdown(&document_data.content);
            let safe_filename = sanitize_filename::sanitize(format!("{}.md", document_data.title));
            let file_path = trove_dir.join(&safe_filename);

            // Get the old title in a separate scope
            let old_title = {
                let tab_switcher = state.tab_switcher.read().unwrap();
                tab_switcher
                    .tabs
                    .get(&document_data.id)
                    .map(|tab| tab.title.clone())
                    .unwrap_or_else(|| String::from("Untitled"))
            };

            let old_path = trove_dir.join(sanitize_filename::sanitize(format!("{}.md", old_title)));

            // if the title has changed, delete the old file
            if old_path != file_path && old_path.exists() {
                let _ = fs::remove_file(old_path)
                    .map_err(|e| format!("Failed to delete old file: {}", e));
            }

            let _ = if let Err(e) = fs::write(&file_path, markdown_content) {
                Err(format!("Failed to write file: {}", e))
            } else {
                Ok(file_path.to_string_lossy().to_string())
            };
        }
    }

    ///TODO: The delete_document command needs to be worked on to support
    ///the new state management and remove all the legacy code.
    pub fn delete_document(app: AppHandle, _payload: Option<String>) {
        log::debug!("delete_document init");
        let temp_app = app.clone();
        let state = &temp_app.state::<AppState>();
        let orig_state = &state;

        let current_tab_id = {
            let tabswitcher = state.tab_switcher.read().unwrap();
            //TODO: Handle the case where the current_tab_id can be none!
            let current_tab_id = tabswitcher.current_tab_id.clone().unwrap();
            current_tab_id
        };

        // Get the tab information in a separate scope
        let (tab_title, next_tab_info) = {
            let tabswitcher = state.tab_switcher.read().unwrap();
            if !tabswitcher.tabs.contains_key(&current_tab_id) {
                log::debug!("Tab not found.");
                return;
            }

            let title = tabswitcher
                .tabs
                .get(&current_tab_id)
                .map(|tab| tab.title.clone())
                .unwrap();
            let next = tabswitcher
                .tabs
                .get_index(0)
                .or_else(|| tabswitcher.tabs.last())
                .map(|(next_id, next_tab)| (next_id.clone(), next_tab.title.clone()));

            (title, next)
        };

        // Update tab switcher in a separate scope
        {
            let mut tabswitcher = state.tab_switcher.write().unwrap();
            tabswitcher.tabs.shift_remove(&current_tab_id);
            if let Some((next_id, _)) = &next_tab_info {
                tabswitcher.current_tab_id = Some(next_id.clone());
            }
        }
        update_tabs_state(app.clone());

        // Handle file operations
        let trove_dir = get_trove_dir("Untitled_Trove");
        let filename = sanitize_filename::sanitize(format!("{}.md", tab_title));
        let file_path = trove_dir.join(&filename);

        if file_path.exists() {
            let _ = fs::remove_file(&file_path)
                .map_err(|e| format!("Failed to delete file {}: {}", file_path.display(), e));
        }

        // Update recent files in a separate scope to avoid deadlocks.
        {
            let mut workspace = state.workspace.write().unwrap();
            workspace
                .recent_files
                .retain(|doc| doc.id != current_tab_id);
        }

        let _ = save_user_data(orig_state);

        // Get the content for the next tab
        let next_tab = if let Some((next_id, next_title)) = next_tab_info {
            //TODO: Handle panic cases here when using unwrap.
            get_document_content(next_id, next_title).unwrap()
        } else {
            None
        };
        //TODO: Handle panic cases here when using unwrap.
        let _ = app.emit("current_editor_content", next_tab.unwrap().content);
    }

    //TODO: Cleanup unused variables.
    pub fn get_document_content(app: AppHandle, payload: Option<String>) {
        let Some(payload) = payload else {
            log::warn!("Invalid call to save_document");
            return;
        };
        // let temp_app = app.clone();
        // let state = &temp_app.state::<AppState>();

        if let Ok(tab_data) = serde_json::from_str::<Tab>(&payload) {
            // let id = tab_data.id;
            let title = tab_data.title;

            // Get the path of the document using title
            let trove_dir = get_trove_dir("Untitled_Trove");
            let file_path = trove_dir.join(format!("{}.md", title));

            // Check if the file exists
            if !file_path.exists() {
                // If the file does not exist, return None
                return;
            }

            // Read the file content using the file path
            match fs::read_to_string(&file_path) {
                // If the file is read successfully, convert the markdown content to HTML
                Ok(content) => {
                    let html_output = markdown_handler::markdown_to_html(&content);

                    // Update the current content on the screen.
                    let _ = app.emit("current_editor_content", html_output);
                    //     Ok(Some(DocumentData {
                    //         id,
                    //         title,
                    //         content: html_output,
                    //     }))
                }
                // If there is an error in reading the file, return the error
                Err(_e) => (),
            }
        }
    }

    pub fn load_last_open_tabs(app: AppHandle, _payload: Option<String>) {
        log::debug!("load_last_open_tabs init");
        let temp_app = app.clone();
        let state = &temp_app.state::<AppState>();

        let appdata_dir = get_documents_dir().join("appdata");
        let userdata_path = appdata_dir.join("userdata.json");
    
        if userdata_path.exists() {
            match fs::read_to_string(&userdata_path) {
                Ok(content) => match serde_json::from_str::<UserData>(&content) {
                    Ok(user_data) => {
                        // Update workspace in a separate scope
                        {
                            let mut workspace = state.workspace.write().unwrap();
                            workspace.recent_files = user_data.recent_files.clone();
                        }
    
                        // Update tab switcher in a separate scope
                        {
                            let mut tabswitcher = state.tab_switcher.write().unwrap();
                            tabswitcher.current_tab_id = Some(user_data.last_open_tab.clone());
    
                            // Clear existing tabs and load from user_data
                            let tabs = &mut tabswitcher.tabs;
                            tabs.clear();
                            //tabswitcher.tabs = user_data.tabs.clone();
                            for tab in user_data.tabs {
                                tabswitcher.tabs.insert(tab.id.clone(), tab.clone());
                            }
        
                        }
                    }
                    Err(e) => log::debug!("{}", format!("Failed to deserialize userdata: {}", e)),
                },
                Err(e) => log::debug!("{}", format!("Failed to read userdata file: {}", e)),
            }
        }
        update_tabs_state(app.clone());
    }
}

impl CommandRegistrar for IOCommands {
    fn register_commands(registry: &mut CommandRegistry) {
        registry.add_command("save_document".to_string(), Box::new(Self::save_document));
        registry.add_command(
            "delete_document".to_string(),
            Box::new(Self::delete_document),
        );
        registry.add_command(
            "get_document_content".to_string(),
            Box::new(Self::get_document_content),
        );
        registry.add_command(
            "load_last_open_tabs".to_string(),
            Box::new(Self::load_last_open_tabs),
        );
    }
}

/// This function returns the path to the documents directory.
pub fn get_documents_dir() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        // On Android, use the app's private storage directory
        let path = PathBuf::from("/data/user/0/com.rhyolite.dev/Rhyolite");
        // Create the directory if it doesn't exist
        fs::create_dir_all(&path).expect("Could not create Rhyolite directory");
        path
    }

    #[cfg(not(target_os = "android"))]
    {
        // Original desktop behavior
        let mut path = dirs::document_dir().expect("Could not find Documents directory");
        path.push("Rhyolite");
        // Create the directory if it doesn't exist
        fs::create_dir_all(&path).expect("Could not create Rhyolite directory");
        path
    }
}

/// This function returns the path to the default trove directory.
pub fn get_trove_dir(trove_name: &str) -> PathBuf {
    //Get the path to documents/Rhyolite.
    let documents_dir = get_documents_dir();

    //Append the default trove name to the 'documents/Rhyolite path'.
    let trove_dir = documents_dir.join(trove_name);

    //Then create the path 'documents/Rhyolite/trove_name' if it does not
    fs::create_dir_all(&trove_dir).expect("Could not create Trove directory");

    //retrun the path of the default trove directory.
    trove_dir
}

/// Runs when the app is closing and saves the user data.
pub fn on_app_close(window: &Window) {
    log::debug!("on_app_close init");
    let state = window.state::<AppState>();
    // let tab_switcher = &mut state.tab_switcher.lock().unwrap();

    let user_data = {
        let tab_switcher = state.tab_switcher.read().unwrap();
        let workspace = state.workspace.read().unwrap();

        UserData {
            tabs: tab_switcher.tabs.values().cloned().collect::<Vec<_>>(),
            last_open_tab: tab_switcher.current_tab_id.clone().unwrap(),
            recent_files: workspace.recent_files.clone(),
        }
    };

    let appdata_dir = get_documents_dir().join("appdata");
    fs::create_dir_all(&appdata_dir).expect("Could not create appdata directory");
    let userdata_path = appdata_dir.join("userdata.json");

    match serde_json::to_string_pretty(&user_data) {
        Ok(json_content) => {
            if let Err(e) = fs::write(userdata_path, json_content) {
                eprintln!("Failed to save userdata: {}", e);
            }
        }
        Err(e) => eprintln!("Failed to serialize userdata: {}", e),
    }
}

/// This function saves the user data to the userdata.json file.
pub fn save_user_data(state: &State<'_, AppState>) -> Result<(), String> {
    let user_data = {
        let tab_switcher = state.tab_switcher.read().unwrap();
        let workspace = state.workspace.read().unwrap();

        UserData {
            tabs: tab_switcher.tabs.values().cloned().collect(),
            last_open_tab: tab_switcher.current_tab_id.clone().unwrap(),
            recent_files: workspace.recent_files.clone(),
        }
    };

    let appdata_dir = get_documents_dir().join("appdata");
    fs::create_dir_all(&appdata_dir).expect("Could not create appdata directory");
    let userdata_path = appdata_dir.join("userdata.json");

    match serde_json::to_string_pretty(&user_data) {
        Ok(json_content) => fs::write(userdata_path, json_content)
            .map_err(|e| format!("Failed to save userdata: {}", e)),
        Err(e) => Err(format!("Failed to serialize userdata: {}", e)),
    }
}

/// This function saves the document.
#[tauri::command]
pub fn save_document(
    id: String,
    title: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    log::debug!("save_document init");

    {
        let mut workspace = state.workspace.write().unwrap();
        if let Some(doc) = workspace.recent_files.iter_mut().find(|doc| doc.id == id) {
            doc.title = title.clone();
        } else {
            workspace.recent_files.push(FileInfo {
                id: id.clone(),
                title: title.clone(),
            });
        }
    }

    let trove_dir = get_trove_dir("Untitled_Trove");
    let markdown_content = markdown_handler::html_to_markdown(&content);
    let safe_filename = sanitize_filename::sanitize(format!("{}.md", title));
    let file_path = trove_dir.join(&safe_filename);

    // Get the old title in a separate scope
    let old_title = {
        let tab_switcher = state.tab_switcher.read().unwrap();
        tab_switcher
            .tabs
            .get(&id)
            .map(|tab| tab.title.clone())
            .unwrap_or_else(|| String::from("Untitled"))
    };

    let old_path = trove_dir.join(sanitize_filename::sanitize(format!("{}.md", old_title)));

    // if the title has changed, delete the old file
    if old_path != file_path && old_path.exists() {
        fs::remove_file(old_path).map_err(|e| format!("Failed to delete old file: {}", e))?;
    }

    match fs::write(&file_path, markdown_content) {
        Ok(_) => Ok(file_path.to_string_lossy().to_string()),
        Err(e) => Err(format!("Failed to write file: {}", e)),
    }
}

/// This function gets the content of the document by its id and title.
#[tauri::command]
pub fn get_document_content(id: String, title: String) -> Result<Option<DocumentData>, String> {
    // Get the path of the document using title
    let trove_dir = get_trove_dir("Untitled_Trove");
    let file_path = trove_dir.join(format!("{}.md", title));

    // Check if the file exists
    if !file_path.exists() {
        // If the file does not exist, return None
        return Ok(None);
    }

    // Read the file content using the file path
    match fs::read_to_string(&file_path) {
        // If the file is read successfully, convert the markdown content to HTML
        Ok(content) => {
            let html_output = markdown_handler::markdown_to_html(&content);

            // Return the document data as Some(DocumentData)
            Ok(Some(DocumentData {
                id,
                title,
                content: html_output,
            }))
        }
        // If there is an error in reading the file, return the error
        Err(e) => Err(format!("Failed to read file: {}", e)),
    }
}

/// This function loads the tabs active/opened in the last app section.
#[tauri::command]
pub fn load_last_open_tabs(state: State<'_, AppState>) -> Result<Vec<DocumentData>, String> {
    log::debug!("load_last_open_tabs init");
    let appdata_dir = get_documents_dir().join("appdata");
    let userdata_path = appdata_dir.join("userdata.json");

    if userdata_path.exists() {
        match fs::read_to_string(&userdata_path) {
            Ok(content) => match serde_json::from_str::<UserData>(&content) {
                Ok(user_data) => {
                    // Update workspace in a separate scope
                    {
                        let mut workspace = state.workspace.write().unwrap();
                        workspace.recent_files = user_data.recent_files.clone();
                    }

                    // Update tab switcher in a separate scope
                    {
                        let mut tabswitcher = state.tab_switcher.write().unwrap();
                        tabswitcher.current_tab_id = Some(user_data.last_open_tab.clone());

                        // Clear existing tabs and load from user_data
                        let tabs = &mut tabswitcher.tabs;
                        tabs.clear();
                    }

                    let mut last_open_files = Vec::new();

                    // Process tabs and load documents
                    for tab in user_data.tabs {
                        match get_document_content(tab.id.clone(), tab.title.clone()) {
                            Ok(Some(doc)) => {
                                last_open_files.push(doc);
                                let mut tabswitcher = state.tab_switcher.write().unwrap();
                                tabswitcher.tabs.insert(tab.id.clone(), tab.clone());
                            }
                            _ => continue,
                        }
                    }

                    return Ok(last_open_files);
                }
                Err(e) => return Err(format!("Failed to deserialize userdata: {}", e)),
            },
            Err(e) => return Err(format!("Failed to read userdata file: {}", e)),
        }
    }

    // If userdata.json doesn't exist, load all markdown files from the trove directory
    let trove_dir = get_trove_dir("Untitled_Trove");

    let files = match fs::read_dir(&trove_dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "md"))
            .filter_map(|entry| {
                let path = entry.path();
                let title = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(String::from)
                    .unwrap_or_default();

                let id = Uuid::new_v4().to_string();
                get_document_content(id, title).ok().flatten()
            })
            .collect(),
        Err(e) => return Err(format!("Failed to read directory: {}", e)),
    };

    Ok(files)
}

/// This function returns the metadata of the recent files.
#[tauri::command]
pub fn get_recent_files_metadata(state: State<'_, AppState>) -> Result<Vec<FileInfo>, String> {
    if let Err(e) = save_user_data(&state) {
        eprintln!("Warning: Failed to save user data: {}", e);
    }
    let appdata_dir = get_documents_dir().join("appdata");
    let userdata_path = appdata_dir.join("userdata.json");

    // Check if userdata.json exists
    if userdata_path.exists() {
        // Read and deserialize the UserData
        match fs::read_to_string(&userdata_path) {
            Ok(content) => match serde_json::from_str::<UserData>(&content) {
                Ok(user_data) => Ok(user_data.recent_files),
                Err(e) => Err(format!("Failed to deserialize userdata: {}", e)),
            },
            Err(e) => Err(format!("Failed to read userdata file: {}", e)),
        }
    } else {
        // If userdata.json doesn't exist, return empty vector
        Ok(Vec::new())
    }
}
