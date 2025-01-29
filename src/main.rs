use anyhow::Context as _;
use serenity::all::{ChannelId, Http};
use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::prelude::*;
use shuttle_runtime::SecretStore;
use sqlx::PgPool;
use std::time::Duration;
use tracing::info;

pub mod llm;

struct Bot {
    pool: PgPool,
}

impl Bot {
    fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EventHandler for Bot {
    async fn message(&self, _: Context, msg: Message) {
        // note we can basically garuantee this will be a JSON compatible
        // object, so we can unwrap here while developing
        let message = serde_json::to_string_pretty(&msg).unwrap();
        sqlx::query("INSERT INTO messages (data) values ($1)")
            .bind(message)
            .execute(&self.pool)
            .await
            .unwrap();
    }

    async fn ready(&self, _: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);
    }
}

#[shuttle_runtime::main]
async fn serenity(
    #[shuttle_runtime::Secrets] secrets: SecretStore,
    #[shuttle_shared_db::Postgres] pool: PgPool,
) -> shuttle_serenity::ShuttleSerenity {
    secrets.into_iter().for_each(|(key, val)| {
        std::env::set_var(key, val);
    });
    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("Couldn't run database migrations");
    // Get the discord token set in `Secrets.toml`
    let token = std::env::var("DISCORD_TOKEN").context("'DISCORD_TOKEN' was not found")?;
    let channel_id: ChannelId = std::env::var("CHANNEL_ID")
        .context("'CHANNEL_ID' was not found")?
        .parse::<u64>()
        .context("Tried to convert CHANNEL_ID env var but the value is not a valid u64")?
        .into();

    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;

    let client = serenity::Client::builder(&token, intents)
        .event_handler(Bot::new(pool.clone()))
        .await
        .expect("Err creating client");

    tokio::spawn(async move {
        automated_summarized_messages(channel_id, token, pool).await;
    });

    Ok(client.into())
}

pub async fn automated_summarized_messages(channel_id: ChannelId, token: String, pool: PgPool) {
    let http_client = Http::new(&token);
    let mut interval = tokio::time::interval(Duration::from_secs(86400));
    loop {
        interval.tick().await;

        let report = match generate_report(&pool).await {
            Ok(res) => res,
            Err(e) => {
                println!("{e}");
                continue;
            }
        };

        if let Err(err) = http_client
            .send_message(channel_id, Vec::new(), &report)
            .await
        {
            println!("Something went wrong while sending summary message: {err}");
        };
    }
}

pub async fn generate_report(pool: &PgPool) -> Result<String, Box<dyn std::error::Error>> {
    let date_yesterday = chrono::Utc::now().date_naive() - chrono::Days::new(1);
    let res: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT jsonb_agg(data) FROM messages WHERE created::date = $1")
            .bind(date_yesterday)
            .fetch_optional(pool)
            .await?;

    let Some(res) = res else {
        return Err("There were no messages in the database :(".into());
    };

    let raw_json = serde_json::to_string_pretty(&res).unwrap();

    let prompt_result = match llm::summarize_messages(raw_json).await {
        Ok(res) => res,
        Err(e) => {
            return Err(
                format!("Something went wrong while trying to summarize messages: {e}").into(),
            )
        }
    };

    if let Err(e) = sqlx::query("INSERT INTO summaries (summary, date) VALUES ($1, $2)")
        .bind(&prompt_result)
        .bind(date_yesterday)
        .execute(pool)
        .await
    {
        return Err(format!("Error ocurred while storing summary: {e}").into());
    };

    Ok(prompt_result)
}
