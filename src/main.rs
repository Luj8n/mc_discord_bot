use mc_query::rcon::RconClient;
use serde::Deserialize;
use serenity::async_trait;
use serenity::model::prelude::application_command::CommandDataOptionValue;
use serenity::model::prelude::*;
use serenity::prelude::*;
use serenity::utils::Colour;
use std::time::Duration;
use std::{env, io};
use tokio::time;

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum MojangResponse {
  Success {
    id: String,
    name: String,
  },
  Failure {
    path: String,
    #[serde(rename = "errorMessage")]
    error_message: String,
  },
}

/// Returns the uuid of the provided username using the official mojang api.
/// Returns `None` if there was a network error, or that player doesn't exist
async fn get_mojang_profile(username: &str) -> Option<MojangResponse> {
  reqwest::get(format!(
    "https://api.mojang.com/users/profiles/minecraft/{}",
    username
  ))
  .await
  .ok()?
  .json::<MojangResponse>()
  .await
  .ok()
}

struct Handler {
  server_address: String,
  rcon_password: String,
  status_channel_id: u64,
  verify_channel_id: u64,
}

async fn create_rcon_client(server_address: &str, rcon_password: &str) -> io::Result<RconClient> {
  let mut rcon_client = RconClient::new(server_address, 25575).await?;

  rcon_client.authenticate(rcon_password).await?;

  Ok(rcon_client)
}

impl Handler {
  async fn new() -> Self {
    let server_address =
      env::var("SERVER_ADDRESS").expect("Expected SERVER_ADDRESS in the environment variables");

    let rcon_password =
      env::var("RCON_PASSWORD").expect("Expected RCON_PASSWORD in the environment variables");

    let status_channel_id: u64 = env::var("DISCORD_STATUS_CHANNEL_ID")
      .expect("Expected DISCORD_STATUS_CHANNEL_ID in the environment variables")
      .parse()
      .expect("Couldn't parse DISCORD_STATUS_CHANNEL_ID");

    let verify_channel_id: u64 = env::var("DISCORD_VERIFY_CHANNEL_ID")
      .expect("Expected DISCORD_VERIFY_CHANNEL_ID in the environment variables")
      .parse()
      .expect("Couldn't parse DISCORD_VERIFY_CHANNEL_ID");

    Self {
      server_address,
      rcon_password,
      status_channel_id,
      verify_channel_id,
    }
  }
}

#[async_trait]
impl EventHandler for Handler {
  async fn message(&self, ctx: Context, new_message: Message) {
    // Delete all new messages that are not sent by the bot in the verify channel
    if new_message.channel_id.0 == self.verify_channel_id
      && new_message.author != ctx.cache.current_user().into()
    {
      new_message
        .delete(&ctx)
        .await
        .expect("Couldn't delete a message");
    }
  }

  async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
    if let Interaction::ApplicationCommand(mut command) = interaction {
      let content = match command.data.name.as_str() {
        "verify" => {
          let username = command
            .data
            .options
            .first()
            .expect("There wasn't an option")
            .resolved
            .as_ref()
            .expect("There wasn't a value");

          let username = match username {
            CommandDataOptionValue::String(str) => str,
            _ => panic!("It should be a String"),
          };

          let verify_channel = ctx
            .cache
            .guild_channel(self.verify_channel_id)
            .expect("There should be channel with the provided DISCORD_VERIFY_CHANNEL_ID");

          let guild = verify_channel
            .guild(&ctx)
            .expect("Couldn't find channel's guild");

          let verified_role = guild
            .role_by_name("Verified")
            .expect("There should a Verified role");

          let is_verified = command
            .user
            .has_role(&ctx, guild.id, verified_role)
            .await
            .expect("Couldn't check if user has role");

          if is_verified {
            "You have already verified a username, please contact an admin if you have verified the wrong username or need to change it.".to_string()
          } else {
            match get_mojang_profile(username).await {
              Some(MojangResponse::Success { name, .. }) => {
                match create_rcon_client(&self.server_address, &self.rcon_password).await {
                  Err(err) => {
                    println!("- Couldn't create an rcon client: {err}");
                    "Could not connect to the minecraft server. Probably because it is offline right now. Try again later"
                      .to_string()
                  }
                  Ok(mut rcon_client) => {
                    let server_response = rcon_client
                      .run_command(&format!("whitelist add {name}"))
                      .await
                      .ok();

                    match server_response {
                      Some(_) => {
                        command
                          .member
                          .as_mut()
                          .expect("There should be a user")
                          .add_role(&ctx, verified_role)
                          .await
                          .expect("Couldn't add Verified role to a user");

                        println!("- '{name}' was successfully added to the whitelist");
                        format!("'{name}' was successfully added to the whitelist!")
                      }
                      None => {
                        "Something went wrong... The server is probably offline right now. Try again when the server is online".to_string()
                      }
                    }
                  }
                }
              }
              Some(MojangResponse::Failure { .. }) => {
                format!("There isn't a Mojang user with '{username}' username. Please try again.")
              }
              None => {
                "Couldn't fetch the profile from the Mojang API. Please try again.".to_string()
              }
            }
          }
        }
        _ => "Not a command".to_string(),
      };

      command
        .create_interaction_response(&ctx, |response| {
          response.interaction_response_data(|message| message.content(content).ephemeral(true))
        })
        .await
        .expect("Couldn't respond to a slash command");
    }
  }

  async fn ready(&self, ctx: Context, ready: Ready) {
    println!("- {} is connected!", ready.user.name);

    // If you don't wait at least a little, it doesn't properly work
    println!("- Loading everything...");
    time::sleep(Duration::from_secs(3)).await;

    let verify_channel = ctx
      .cache
      .guild_channel(self.verify_channel_id)
      .expect("There should be channel with the provided DISCORD_VERIFY_CHANNEL_ID");

    let guild = verify_channel
      .guild(&ctx)
      .expect("Couldn't find channel's guild");

    // Create a Verified role if it doesn't exist
    if guild.role_by_name("Verified").is_none() {
      guild
        .create_role(&ctx, |role| {
          role
            .name("Verified")
            .colour(Colour::BLUE.0 as u64)
            .hoist(true)
        })
        .await
        .expect("Couldn't create a role");
      println!("- Created the Verified role");
    }

    // Send the verify info message if the channel has no messages
    if verify_channel
      .messages(&ctx, |x| x.limit(1))
      .await
      .expect("Couldn't get messages of verify channel")
      .is_empty()
    {
      verify_channel
        .send_message(&ctx, |create_message| {
          create_message.embed(|e| {
            e.title("Verification Ready!")
              .description(
                "Type `/verify <username>` to add your minecraft profile to the server whitelist.",
              )
              .footer(|f| f.text("Minecraft Verification Bot"))
              .colour(Colour::DARK_GREEN)
          })
        })
        .await
        .expect("Couldn't send embed");
      println!("- Sent the first verify info message");
    }

    let mut status_channel = ctx
      .cache
      .guild_channel(self.status_channel_id)
      .expect("There should be channel with the provided DISCORD_STATUS_CHANNEL_ID");

    // Add slash commands
    guild
      .set_application_commands(&ctx, |commands| {
        commands.create_application_command(|command| {
          command
            .name("verify")
            .create_option(|option| {
              option
                .name("username")
                .description("Your Minecraft username")
                .kind(command::CommandOptionType::String)
                .required(true)
            })
            .description("Verify a Minecraft username and add it to the whitelist.")
        })
      })
      .await
      .expect("Couldn't create commands");

    // Loop every 5 minutes and update the channel name to the current player count of the minecraft server
    let mut interval = time::interval(Duration::from_secs(5 * 60));

    loop {
      interval.tick().await;

      let status = mc_query::status(&self.server_address, 25565).await;

      let new_channel_name = match status {
        Ok(status) => {
          format!("🎮 Players online: {} 🎮", status.players.online)
        }
        Err(error) => {
          println!("- Couldn't get status. Reason: {}", error);
          "🛑 Server offline 🛑".to_string()
        }
      };

      let old_channel_name = status_channel.name.clone();

      // Only change the channel name if the the new channel name will be different
      if old_channel_name != new_channel_name {
        println!("- Changing channel name...");
        status_channel
          .edit(&ctx, |x| x.name(&new_channel_name))
          .await
          .expect("Couldn't change the name of the channel");
        println!("- Channel name changed from '{old_channel_name}' to '{new_channel_name}'");
      }

      println!(
        "- [{}] Tick complete",
        chrono::Local::now().format("%H:%M:%S")
      );
    }
  }
}

#[tokio::main]
async fn main() {
  // TODO: update serenity
  // TODO?: create a different thread for the interval channel

  dotenvy::dotenv().unwrap();

  let handler = Handler::new().await;

  let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment variables");
  let intents = GatewayIntents::all();

  let mut client = Client::builder(&token, intents)
    .event_handler(handler)
    .await
    .expect("Error creating client");

  if let Err(error) = client.start().await {
    println!("Client error: {:?}", error);
  }
}
