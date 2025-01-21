use std::env;
use rig::completion::Prompt;

pub async fn summarize_messages(messages_json: String) -> Result<String, Box<dyn std::error::Error>> {
    // Create OpenAI client
    let client = rig::providers::openai::Client::new(
        &env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set"),
    );

    // Create agent with a single context prompt
    let summarizer_agent = client
        .agent("gpt-4o")
        .preamble("Your job is to summarize a list of Discord messages from a single day in JSON format.

            The output should be in Markdown and is intended to provide a summary of important events and conversation topics from the day given.

            If there are no messages, simply respond \"Nothing was discussed.\"")
        .build();

    let result = summarizer_agent.prompt(&messages_json).await?;

    Ok(result)
}
