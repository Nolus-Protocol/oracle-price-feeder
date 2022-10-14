use std::str::FromStr;

use cosmrs::{
    AccountId,
    bip32::{DerivationPath, Language, Mnemonic},
    crypto::secp256k1::{Signature, SigningKey},
};

use super::error::WalletError;

/// Represents a Secp256k1 key pair.
pub struct Keychain {
    pub public_key: cosmrs::crypto::PublicKey,
    private_key: SigningKey,
}

/// Facility used to manage a Secp256k1 key pair and generate signatures.
pub struct Wallet {
    keychain: Keychain,
}

impl Wallet {
    pub fn new(mnemonic_phrase: &str, derivation_path: &str) -> Result<Wallet, WalletError> {
        let mnemonic = Mnemonic::new(mnemonic_phrase, Language::English)?;

        let derivation_path = DerivationPath::from_str(derivation_path)
            .map_err(|_| WalletError::DerivationPath(String::from(derivation_path)))?;

        //TODO: password as argument
        let seed = &mnemonic.to_seed("");

        let sender_private_key = SigningKey::derive_from_path(seed, &derivation_path)?;

        let sender_public_key = sender_private_key.public_key();

        Ok(Wallet {
            keychain: Keychain {
                public_key: sender_public_key,
                private_key: sender_private_key,
            },
        })
    }

    pub fn get_sender_account_id(&self, prefix: &str) -> Result<AccountId, WalletError> {
        self.keychain
            .public_key
            .account_id(prefix)
            .map_err(Into::into)
    }

    pub fn get_public_key(&self) -> cosmrs::crypto::PublicKey {
        self.keychain.public_key
    }

    pub fn sign(&self, data: &[u8]) -> Result<Vec<u8>, WalletError> {
        // Sign the data provided data
        self.keychain
            .private_key
            .sign(data).as_ref()
            .map(Signature::to_vec)
            .map_err(|err| WalletError::Sign(err.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::Wallet;

    #[test]
    fn get_address_from_mnemonic() {
        let mnemonic_phrase = "glimpse drama thing brand detail frame spin boss warm people river echo situate creek decorate inhale leaf illness rose order project pear ball stick";
        let derivation_path: &str = "m/44'/118'/0'/0/0";

        let wallet = Wallet::new(mnemonic_phrase, derivation_path).unwrap();
        assert_eq!(
            wallet.get_sender_account_id("unolus").unwrap().to_string(),
            "unolus1j522qf8ewdj42emzlasppmyuxzg53keuq5jd7k"
        )
    }
}
