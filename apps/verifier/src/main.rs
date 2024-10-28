use actix_web::{middleware, web, App, HttpResponse, HttpServer};
use aligned_sdk::core::types::{Network, PriceEstimate, ProvingSystemId, VerificationData};
use aligned_sdk::sdk::{estimate_fee, get_next_nonce, submit_and_wait_verification};
use dotenv::dotenv;
use ethers::signers::{LocalWallet, Signer};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::env;
use std::str::FromStr;

const BATCHER_URL: &str = "wss://batcher.alignedlayer.com";
const NETWORK: Network = Network::Holesky;

#[derive(Serialize, Deserialize)]
struct VerificationRequest {
    proof: Vec<u8>,
    verification_key: Vec<u8>,
    pub_input: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
struct VerificationResponse {
    success: bool,
    error: Option<String>,
}

async fn verify_proof(
    verification_data: VerificationData,
    rpc_url: &str,
    wallet: &LocalWallet,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let nonce = get_next_nonce(rpc_url, wallet.address(), NETWORK)
        .await
        .expect("Failed to get next nonce");
    let max_fee = estimate_fee(&rpc_url, PriceEstimate::Default)
        .await
        .expect("failed to fetch gas price from the blockchain");

    match submit_and_wait_verification(
        BATCHER_URL,
        &rpc_url,
        NETWORK,
        &verification_data,
        max_fee,
        wallet.clone(),
        nonce,
    )
    .await
    {
        Ok(aligned_verification_data) => {
            let batch_root = hex::encode(aligned_verification_data.batch_merkle_root);
            info!("Verification successful. Batch root: {}", batch_root);
            Ok(None)
        }
        Err(e) => {
            error!("Verification error: {}", e);
            Ok(Some(e.to_string()))
        }
    }
}

async fn verify_handler(
    req: web::Json<VerificationRequest>,
    wallet: web::Data<LocalWallet>,
    rpc_url: web::Data<String>,
) -> Result<HttpResponse, actix_web::Error> {
    let req = req.into_inner();

    let verification_data = VerificationData {
        proving_system: ProvingSystemId::Groth16Bn254,
        proof: req.proof,
        pub_input: req.pub_input,
        verification_key: req.verification_key,
        vm_program_code: None,
        proof_generator_addr: wallet.address(),
    };

    match verify_proof(verification_data, &rpc_url, &wallet).await {
        Ok(error) => Ok(HttpResponse::Ok().json(VerificationResponse {
            success: error.is_none(),
            error,
        })),
        Err(e) => Ok(
            HttpResponse::InternalServerError().json(VerificationResponse {
                success: false,
                error: Some(e.to_string()),
            }),
        ),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();

    let rpc_url = env::var("RPC_URL").expect("RPC_URL must be set");
    let private_key = env::var("PRIVATE_KEY").expect("PRIVATE_KEY must be set");

    let wallet = LocalWallet::from_str(&private_key)
        .expect("Failed to create wallet")
        .with_chain_id(17000u64);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(wallet.clone()))
            .app_data(web::Data::new(rpc_url.clone()))
            .wrap(middleware::Logger::default())
            .service(web::resource("/verify").route(web::post().to(verify_handler)))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, web, App};
    use base64::{engine::general_purpose, Engine as _};

    #[actix_web::test]
    async fn test_verify_handler() {
        dotenv().ok();

        let rpc_url = env::var("RPC_URL").expect("RPC_URL must be set");
        let private_key = env::var("PRIVATE_KEY").expect("PRIVATE_KEY must be set");

        let wallet = LocalWallet::from_str(&private_key)
            .expect("Failed to create wallet")
            .with_chain_id(17000u64);

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(wallet.clone()))
                .app_data(web::Data::new(rpc_url.clone()))
                .service(web::resource("/verify").route(web::post().to(verify_handler))),
        )
        .await;

        let proof = general_purpose::STANDARD.decode("A3e/b1SanJb3069LEXBj6SZxSYGoDVuZW2hTiQqMOu8C1u3rlWQH0tnInidCM+3HL5qCrt98x/dJUDAv/3bYaBuTl4T7GKiH0s0nJWAs/PwJYEAl+2VOXdSxccwrFzyFBumH4iBPXio2L8gRBuk4NZoO28g2YtvdT4wdXBI0OloKJyJDCQ7pX9LN2WmJOuaSiBIQUU/0lqZ2xuN4vzYgkQbUIm3dg6JjbDh40gJGJnsOSut/bjuIUykYTbC5YOE+Aj43hxm+0xJDd3FniP5KBTHaZWJq3uYUY42PkEMQ+RQfqqIsbkKdLcTjnhb6PmzR1wvuqSfGhsZbShoENbHqhQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
                .expect("Failed to decode proof");

        let verification_key = general_purpose::STANDARD.decode("LTvfOdMDcumyiukLEt0OqatMCrUdGyPv1auq9e4gjC8i43HAlNYg1eSDtDUhet21s0sJVLYgRFp3kknhdpZ9ECa2HBiUu/9QBP2JTi4s7nfrVOndl28AJPbdVlvKkn0eKHP9ROLwigfgVEuRZcB6Zw14eYVo/FN1JTIQKSX6kTQu5wy7l1YQZwe1Rhqv+759f8pPFe2a28a1UCchM0RqnyYLy71gG3IF1Dbyrz/Z+oWqzvun2fdAVoeXT80m0HFJD0qDoVffmXc7ZGHfWQvAfebysaZIcLpDQQFOTaxb3E0QiGiH5dwW+jdUzgq9d88bLL7mOGsTwoEsHp3m5SwL8i8XhP9RxDViZhk26606HKFxJTVVvSU1UUnj4uvtxM01H3MgBPtc5FKhqbkgj3NDC/tB72C6cwlT7WhpMCTQp5ET4VwLA7Og0UMphGoKfy5PV6YDLqNZBXEFEIsxH79ebBf8MkNl7/9z/rqXXUOfWFA+VvCnHTsdcZJXeujYyG2pBWNy6SvMQTI5TvACciNXbyO6+A4+X1sz0E/R8Uz/IgAdnJlbrPW0cLckssE7KvKMutNDaRhKUwLQmXPVtHwPvxC+mxKHFiByu+0l0cTXC9iFX4Zn7/DDxNFmfehlP/Q4FFI0Xd26aYIRNf30LJ+XoILSuDFGL/NAJTNZo/RADLIUJLGGIGg5SpM4BaCAP94Yo9SPoW/R/dR12wW/zXkNsgWxFVihwewNY81XAb0VDouEgVe8N60xJnOFiBJ9rPZEAAAAAhTTv33YX8is/rbX+Ean1pq6TX4nQLZ7pW0b+cguIgE+BdyqgFwgPmbqxy3u1ALT4QisNgTzCS9qYxcyMRKxGR0WKg42If7GjD7ia2ea1DumFhp2VvRYwMvcCajv7u5IwCmgpFezLb2hddCp8pJlVDoZC+ueGOIx1AdI9mQ3LTUyAAAAACXTh11m9U0volPV946AzfZbB6KFKMRWmh5UZ75+y5JGI/LtleSEvwKfgQZhmWbfupyFF6PNhe7/9wl0mlOlAbknsYOYcXK8Gk/TyJNsmQ+tSRdG9PVEW9h7iJgWkvNPigj28cTT65Q0jetQNb/qHT3Osg1bGLqLAY8FB+gIAlYcDAZXQNWll/qaxq4QEPKjwQNws3Dr0WQ4h5K1TyR+Yu0Wf5onlGSdROzxG03rPknwF0yguF/r8HIaY1CHAxvXliXKgW0hieGfn3l/4Ynf/HUhm9lkVw1z+tclIyR4p5EiDhrgCPvaJUoUVHOjnzSrVo3HZZXg3Q/ZccLb102NoHE=")
                .expect("Failed to decode verification key");

        let pub_input = vec![18u8];

        let req = VerificationRequest {
            proof,
            verification_key,
            pub_input,
        };

        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/verify")
                .set_json(&req)
                .to_request(),
        )
        .await;

        assert!(resp.status().is_success());

        let body: VerificationResponse = test::read_body_json(resp).await;
        println!("{:?}", body);
        assert!(body.success);
        assert!(body.error.is_none());
    }
}
