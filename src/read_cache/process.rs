use std::sync::{atomic::AtomicBool, Arc};

use console::style;
use mpl_candy_machine_core::replace_patterns;
use reqwest::IntoUrl;

use crate::{
    cache::load_cache,
    common::*,
    config::{get_config_data, HiddenSettings},
    validate::format::Metadata,
};

pub struct ReadCacheArgs {
    pub config: String,
    pub cache: String,
    pub interrupted: Arc<AtomicBool>,
}

pub struct AssetType {
    pub image: Vec<isize>,
    pub metadata: Vec<isize>,
    pub animation: Vec<isize>,
}

pub async fn process_read_cache(args: ReadCacheArgs) -> Result<()> {
    let config_data = get_config_data(&args.config)?;

    // creates/loads the cache
    let mut cache = load_cache(&args.cache, true)?;

    let http_client = reqwest::Client::new();

    if let Some(HiddenSettings { name, uri, .. }) = config_data.hidden_settings {
        for index in 0..config_data.number {
            println!(
                "{} {}Downloading assets",
                style(format!("[{}/{}]", index, config_data.number))
                    .bold()
                    .dim(),
                ASSETS_EMOJI
            );
            let name = replace_patterns(name.clone(), index as usize);
            let metadata_link = replace_patterns(uri.clone(), index as usize);
            let metadata_text = http_client.get(&metadata_link).send().await?.text().await?;
            let metadata: Metadata = match serde_json::from_str(&metadata_text) {
                Ok(metadata) => metadata,
                Err(err) => {
                    let error = anyhow!("Error parsing metadata ({}): {}", &metadata_link, err);
                    error!("{:?}", error);
                    return Err(error);
                }
            };
            if metadata.name != name {
                let error = anyhow!(
                    "Error checking metadata ({}): Invalid name {} expected {}",
                    metadata_link,
                    metadata.name,
                    name
                );
                error!("{:?}", error);
                return Err(error);
            }
            let metadata_hash = encode_text(&metadata_text)?;
            let image_link = metadata.image;
            let image_hash = encode_url(&http_client, &image_link).await?;

            cache.items.insert(
                index.to_string(),
                CacheItem {
                    name,
                    image_hash,
                    image_link,
                    metadata_hash,
                    metadata_link,
                    on_chain: false,      // TODO: think
                    animation_hash: None, // TODO
                    animation_link: None,
                },
            );
        }
    } else {
        let error = anyhow!("Only hidden type of config is supported");
        error!("{:?}", error);
        return Err(error);
    }

    // move all non-numeric keys to the beginning and sort as strings
    // sort numeric keys as integers
    cache
        .items
        .sort_by(|key_a, _, key_b, _| -> std::cmp::Ordering {
            let a = key_a.parse::<i32>();
            let b = key_b.parse::<i32>();

            if a.is_err() && b.is_err() {
                // string, string
                key_a.cmp(key_b)
            } else if a.is_ok() && b.is_err() {
                // number, string
                std::cmp::Ordering::Greater
            } else if a.is_err() && b.is_ok() {
                // string, number
                std::cmp::Ordering::Less
            } else {
                // number, number
                a.unwrap().cmp(&b.unwrap())
            }
        });
    cache.sync_file()?;

    Ok(())
}

pub fn encode_text(text: &str) -> Result<String> {
    use data_encoding::HEXLOWER;
    use ring::digest::{Context, SHA256};
    let mut context = Context::new(&SHA256);
    context.update(text.as_bytes());

    Ok(HEXLOWER.encode(context.finish().as_ref()))
}

pub async fn encode_url(http_client: &HttpClient, url: impl IntoUrl) -> Result<String> {
    use data_encoding::HEXLOWER;
    use futures::StreamExt;
    use ring::digest::{Context, SHA256};

    let mut input = http_client.get(url).send().await?.bytes_stream();
    let mut context = Context::new(&SHA256);
    while let Some(part) = input.next().await {
        context.update(&part?);
    }

    Ok(HEXLOWER.encode(context.finish().as_ref()))
}
