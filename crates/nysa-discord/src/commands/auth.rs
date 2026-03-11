use poise::serenity_prelude as serenity;

#[poise::command(slash_command, description = "Authenticate with Nysa or link accounts")]
pub async fn auth(
    ctx: poise::Context<'_, crate::DiscordExtensionData>,
    #[description = "Your nysa token or linking code"] 
    _token_or_code: Option<String>,
) -> Result<(), serenity::Error> {
    let template = &ctx.framework().user_data().config.unauth_message;
    
    let embed = serenity::CreateEmbed::new()
        .title(&template.title)
        .description(&template.description)
        .color(template.color);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}
