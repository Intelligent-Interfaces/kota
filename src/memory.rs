use anyhow::Result;
use libsql::Builder;

pub struct MemoryStore {
    conn: libsql::Connection,
}

impl MemoryStore {
    pub async fn new(db_path: &str) -> Result<Self> {
        let db = Builder::new_local(db_path).build().await?;
        let conn = db.connect()?;
        
        conn.execute("
            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                mode TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )
        ", ()).await?;

        conn.execute("
            CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (conversation_id) REFERENCES conversations(id)
            )
        ", ()).await?;

        Ok(Self { conn })
    }

    pub async fn save_conversation(&self, id: &str, mode: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO conversations (id, mode) VALUES (?1, ?2)",
            (id.to_string(), mode.to_string()),
        ).await?;
        Ok(())
    }

    pub async fn save_message(&self, conversation_id: &str, role: &str, content: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO messages (conversation_id, role, content) VALUES (?1, ?2, ?3)",
            (conversation_id.to_string(), role.to_string(), content.to_string()),
        ).await?;
        Ok(())
    }

    pub async fn get_recent_messages(&self, conversation_id: &str, limit: usize) -> Result<Vec<(String, Option<String>)>> {
        let mut stmt = self.conn.prepare("
            SELECT role, content FROM (
                SELECT id, role, content FROM messages 
                WHERE conversation_id = ?1 
                ORDER BY timestamp DESC LIMIT ?2
            ) ORDER BY id ASC
        ").await?;
        
        let mut rows = stmt.query((conversation_id.to_string(), limit as i64)).await?;
        let mut messages = Vec::new();
        
        while let Some(row) = rows.next().await? {
            let role: String = row.get(0)?;
            let content: Option<String> = row.get(1)?;
            messages.push((role, content));
        }
        
        Ok(messages)
    }
}
