use std::sync::{Arc, Mutex};

use chatgpt::client::ChatGPT;
use tokio::task::JoinHandle;

pub struct AiQueue {
    pub context: String,
    pub client: Arc<ChatGPT>,
    pub results: Arc<Mutex<Vec<String>>>,
    pub handles: Vec<JoinHandle<()>>,
}

impl AiQueue {
    pub fn gen(&mut self, details: &str, text: &str) {
        let client = self.client.clone();
        let results = self.results.clone();
        let context = self.context.clone();
        let text = text.to_string();
        let details = details.to_string();

        let result_id = {
            let mut results = self.results.lock().unwrap();
            results.push(String::new());
            results.len() - 1
        };

        self.handles.push(tokio::spawn(async move {
			let response = client
			.send_message(format!(
				"You are a documentation generator.
				This is the signature of the value you are modifying, with an explanation of its behavior.
				```lua
				{context}
				```

				Generate a new {details} with excessive detail. Do not copy the original {details}. The current {details} is:
				\"{text}\"
				"
			))
			.await
			.expect("you feeble mortal, GPT does not fail");

			results.lock().unwrap()[result_id] = response.message().content.clone();
        }));
    }
}
