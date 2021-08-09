use rand::{distributions::Alphanumeric, Rng};

pub fn generate_rand_string(length: usize) -> String {
    let lobbyid: String = {
        rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(length)
            .map(char::from)
            .collect()
    };
    lobbyid.to_ascii_uppercase()
}
