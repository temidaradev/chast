use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};
use anyhow::{Result, Context};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub sender: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub message_type: MessageType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    Text,
    File,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub session_id: String,
    pub participants: Vec<String>,
    pub messages: Vec<ChatMessage>,
    pub created_at: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
}

#[derive(Deserialize, Serialize)]
pub struct Config {
    pub auto_save_enabled: bool,
    pub save_directory: String,
    pub max_saved_chats: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            auto_save_enabled: true,
            save_directory: "saved_chats".to_string(),
            max_saved_chats: 50,
        }
    }
}

pub struct ChatSaveManager {
    config: Config,
    save_dir: PathBuf,
}

impl ChatSaveManager {
    pub fn new(config: Config) -> Result<Self> {
        let save_dir = PathBuf::from(&config.save_directory);
        
        if !save_dir.exists() {
            fs::create_dir_all(&save_dir)
                .context("Failed to create save directory")?;
        }
        
        Ok(Self {
            config,
            save_dir,
        })
    }

    pub fn auto_save_chat(&self, session: &ChatSession) -> Result<()> {
        if !self.config.auto_save_enabled {
            return Ok(());
        }

        let filename = format!("chat_{}_{}.json", 
            session.session_id, 
            session.created_at.format("%Y%m%d_%H%M%S")
        );
        let file_path = self.save_dir.join(filename);
        
        self.save_chat_to_file(session, &file_path)?;
        self.cleanup_old_chats()?;
        
        Ok(())
    }

    pub fn save_chat_to_file(&self, session: &ChatSession, path: &Path) -> Result<()> {
        let file = File::create(path)
            .with_context(|| format!("Failed to create file: {}", path.display()))?;
        
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, session)
            .context("Failed to serialize chat session")?;
        
        println!("Chat saved to: {}", path.display());
        Ok(())
    }

    pub fn load_chat_from_file(&self, path: &Path) -> Result<ChatSession> {
        let file = File::open(path)
            .with_context(|| format!("Failed to open file: {}", path.display()))?;
        
        let reader = BufReader::new(file);
        let session: ChatSession = serde_json::from_reader(reader)
            .context("Failed to deserialize chat session")?;
        
        Ok(session)
    }

    pub fn get_saved_chats(&self) -> Result<Vec<ChatFileInfo>> {
        let mut chat_files = Vec::new();
        
        if !self.save_dir.exists() {
            return Ok(chat_files);
        }

        let entries = fs::read_dir(&self.save_dir)
            .context("Failed to read save directory")?;

        for entry in entries {
            let entry = entry.context("Failed to read directory entry")?;
            let path = entry.path();
            
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        let file_info = ChatFileInfo {
                            path: path.clone(),
                            filename: path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("Unknown")
                                .to_string(),
                            last_modified: modified.into(),
                            size: metadata.len(),
                        };
                        chat_files.push(file_info);
                    }
                }
            }
        }

        chat_files.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));
        
        Ok(chat_files)
    }

    pub fn open_file_explorer_for_restore(&self) -> Result<Option<PathBuf>> {
        use rfd::FileDialog;
        
        let file = FileDialog::new()
            .add_filter("JSON Chat Files", &["json"])
            .set_directory(&self.save_dir)
            .set_title("Select Chat File to Restore")
            .pick_file();
            
        Ok(file)
    }

    pub fn restore_chat_interactive(&self) -> Result<Option<ChatSession>> {
        if let Some(path) = self.open_file_explorer_for_restore()? {
            let session = self.load_chat_from_file(&path)?;
            println!("Chat restored from: {}", path.display());
            Ok(Some(session))
        } else {
            Ok(None)
        }
    }

    fn cleanup_old_chats(&self) -> Result<()> {
        let mut chat_files = self.get_saved_chats()?;
        
        if chat_files.len() > self.config.max_saved_chats {
            chat_files.sort_by(|a, b| a.last_modified.cmp(&b.last_modified));
            
            let files_to_remove = chat_files.len() - self.config.max_saved_chats;
            for file_info in chat_files.iter().take(files_to_remove) {
                if let Err(e) = fs::remove_file(&file_info.path) {
                    eprintln!("Failed to remove old chat file {}: {}", 
                        file_info.path.display(), e);
                }
            }
        }
        
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ChatFileInfo {
    pub path: PathBuf,
    pub filename: String,
    pub last_modified: DateTime<Utc>,
    pub size: u64,
}