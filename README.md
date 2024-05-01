# A discord bot for a minecraft server, which does 2 things:
- Checks the status of the server (online or offline) and updates a discord channel's name accordingly (usually it's a locked voice channel).
- Adds a 'verify' command (sends an informational message about the command in a dedicated discord channel) which allows users to add their own minecraft username to the whitelist of the server (can only be done once).

---

## To build/run it, it needs OpenSSL:

- Fedora: `sudo dnf install openssl-devel`
- Others: https://docs.rs/openssl/latest/openssl/#automatic

---

## To run it:

- Create a `.env` file with these values:

```env
DISCORD_TOKEN=[token of the discord bot]
SERVER_ADDRESS=[server address of the minecraft server]
DISCORD_STATUS_CHANNEL_ID=[the (voice) channel id]
DISCORD_VERIFY_CHANNEL_ID=[the text channel id]
RCON_PASSWORD=[rcon password of the minecraft server]
```
- Start the bot

```
cargo run --release
```

---

## Note

- For the whitelisting functionality, RCON has to be enabled in the `server.properties`
