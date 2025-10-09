use risc0_zkvm::{
    guest::{env, sha::Impl},
    sha::{Digest, Sha256},
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct GetPostsResponse {
    posts: Vec<PostView>,
}

#[derive(Debug, Deserialize)]
struct PostView {
    uri: String,
    cid: String,
    author: Author,
    record: PostRecord,
    #[serde(default)]
    embed: Option<Embed>,
    #[serde(rename = "bookmarkCount", default)]
    bookmark_count: u64,
    #[serde(rename = "replyCount", default)]
    reply_count: u64,
    #[serde(rename = "repostCount", default)]
    repost_count: u64,
    #[serde(rename = "likeCount", default)]
    like_count: u64,
    #[serde(rename = "quoteCount", default)]
    quote_count: u64,
    #[serde(rename = "indexedAt")]
    indexed_at: String,
    #[serde(default)]
    labels: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct Author {
    did: String,
    handle: String,
    #[serde(rename = "displayName", default)]
    display_name: Option<String>,
    #[serde(default)]
    avatar: Option<String>,
    #[serde(default)]
    associated: Option<serde_json::Value>,
    #[serde(default)]
    labels: Vec<serde_json::Value>,
    #[serde(rename = "createdAt", default)]
    created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PostRecord {
    #[serde(rename = "$type")]
    record_type: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    text: String,
    #[serde(default)]
    langs: Vec<String>,
    #[serde(default)]
    embed: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct Embed {
    #[serde(rename = "$type")]
    embed_type: String,
    #[serde(default)]
    images: Vec<serde_json::Value>,
}

fn main() {
    // Read post size (8 bytes)
    let mut post_size_bytes = [0u8; 8];
    env::read_slice(&mut post_size_bytes);
    let _post_size = u64::from_be_bytes(post_size_bytes);
    println!("Input: {:?}", _post_size);
    
    // Read keywords string size (8 bytes)
    let mut keywords_size_bytes = [0u8; 8];
    env::read_slice(&mut keywords_size_bytes);
    let keywords_size = u64::from_be_bytes(keywords_size_bytes) as usize;
    println!("Keywords size: {:?}", keywords_size);
    
    // Read keywords string
    let mut keywords_bytes = vec![0u8; keywords_size];
    env::read_slice(&mut keywords_bytes);
    let keywords_string = String::from_utf8(keywords_bytes).unwrap_or_default();
    println!("Keywords string: {:?}", keywords_string);
    
    // Parse comma-separated keywords
    let keywords: Vec<String> = keywords_string
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    
    // Read URL response size (8 bytes)
    // let mut url_size_bytes = [0u8; 8];
    // env::read_slice(&mut url_size_bytes);
    // let url_size = u64::from_be_bytes(url_size_bytes) as usize;
    
    // Read URL response (Bluesky API JSON)
    let mut url_response = vec![0u8; _post_size as usize];
    env::read_slice(&mut url_response);

    let digest = Impl::hash_bytes(&[url_response.as_slice()].concat());
    env::commit_slice(digest.as_bytes());
    
    // Parse JSON
    let api_response: GetPostsResponse = match serde_json::from_slice(&url_response) {
        Ok(r) => r,
        Err(_) => {
            env::commit(&0u8);
            return;
        }
    };
    println!("API Response: {:?}", api_response);
    
    // Get post
    let post = match api_response.posts.into_iter().next() {
        Some(p) => p,
        None => {
            env::commit(&0u8);
            return;
        }
    };
    println!("Post: {:?}", post);
    
    // Extract text from record
    let post_text = post.record.text;
    println!("Post text: {:?}", post_text);
    
    // Verify keywords
    let post_text_lower = post_text.to_lowercase();
    let mut has_keyword = keywords.is_empty();
    
    for keyword in &keywords {
        if post_text_lower.contains(&keyword.to_lowercase()) {
            has_keyword = true;
            break;
        }
    }
    
    // Return result
    let result = if has_keyword { 1u8 } else { 0u8 };

    println!("Result: {:?}", result);
    env::commit(&result);
}
