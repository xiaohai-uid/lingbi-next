use ed25519_dalek::{Signature, VerifyingKey};
use lingbi_contracts::{AppError, ErrorCode};

pub struct UpdateSignatureVerifier {
    key: VerifyingKey,
}

impl UpdateSignatureVerifier {
    pub fn new(public_key: [u8; 32]) -> Result<Self, AppError> {
        let key = VerifyingKey::from_bytes(&public_key).map_err(|error| {
            AppError::new(
                ErrorCode::EntitlementInvalid,
                format!("invalid updater public key: {error}"),
                false,
            )
        })?;
        Ok(Self { key })
    }

    pub fn verify(&self, manifest: &[u8], signature: &[u8]) -> bool {
        let Ok(signature) = Signature::from_slice(signature) else {
            return false;
        };
        self.key.verify_strict(manifest, &signature).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn keypair() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    #[test]
    fn valid_update_signature_is_accepted() {
        let signing = keypair();
        let manifest = b"version=0.2.0";
        let signature = signing.sign(manifest);
        let verifier =
            UpdateSignatureVerifier::new(signing.verifying_key().to_bytes()).expect("verifier");

        assert!(verifier.verify(manifest, &signature.to_bytes()));
    }

    #[test]
    fn tampered_update_manifest_is_rejected() {
        let signing = keypair();
        let manifest = b"version=0.2.0";
        let signature = signing.sign(manifest);
        let verifier =
            UpdateSignatureVerifier::new(signing.verifying_key().to_bytes()).expect("verifier");

        assert!(!verifier.verify(b"version=0.2.0-tampered", &signature.to_bytes()));
    }
}
