use crate::server::{LinkSocksServer, ReverseTokenOptions, StatusSnapshot, TokenSnapshot};
use hyper::{Body, Method, Request, Response, StatusCode};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;

/// API handler for LinkSocksServer
pub struct ApiHandler {
    server: Arc<LinkSocksServer>,
    api_key: String,
}

/// API response
#[derive(Serialize, Deserialize)]
struct ApiResponse<T> {
    success: bool,
    error: Option<String>,
    data: Option<T>,
}

/// Token information
#[derive(Serialize, Deserialize)]
struct TokenInfo {
    token: String,
    port: Option<u16>,
    client_count: usize,
}

/// Server status
#[derive(Serialize, Deserialize)]
struct ServerStatus {
    client_count: usize,
    forward_token_count: usize,
    reverse_token_count: usize,
    connector_token_count: usize,
}

/// Add token request
#[derive(Serialize, Deserialize)]
struct AddTokenRequest {
    token: Option<String>,
    port: Option<u16>,
    username: Option<String>,
    password: Option<String>,
    allow_manage_connector: Option<bool>,
    #[serde(default)]
    reverse: bool,
}

/// Add connector request
#[derive(Serialize, Deserialize)]
struct AddConnectorRequest {
    connector_token: Option<String>,
    reverse_token: String,
}

impl ApiHandler {
    pub fn new(server: Arc<LinkSocksServer>, api_key: String) -> Self {
        ApiHandler { server, api_key }
    }

    pub async fn handle_request(&self, req: Request<Body>) -> Result<Response<Body>, Infallible> {
        let auth_header = req.headers().get("X-API-Key");
        if auth_header.is_none()
            || auth_header
                .and_then(|value| value.to_str().ok())
                .filter(|value| *value == self.api_key)
                .is_none()
        {
            return self.json(
                StatusCode::UNAUTHORIZED,
                ApiResponse::<()> {
                    success: false,
                    error: Some("Invalid API key".to_string()),
                    data: None,
                },
            );
        }

        match (req.method(), req.uri().path()) {
            (&Method::GET, "/api/status") => self.handle_status().await,
            (&Method::GET, "/api/tokens") => self.handle_list_tokens().await,
            (&Method::POST, "/api/tokens") => self.handle_add_token(req).await,
            (&Method::DELETE, path) if path.starts_with("/api/tokens/") => {
                self.handle_remove_token(path.trim_start_matches("/api/tokens/"))
                    .await
            }
            (&Method::POST, "/api/connectors") => self.handle_add_connector(req).await,
            (&Method::DELETE, path) if path.starts_with("/api/connectors/") => {
                self.handle_remove_connector(path.trim_start_matches("/api/connectors/"))
                    .await
            }
            _ => self.json(
                StatusCode::NOT_FOUND,
                ApiResponse::<()> {
                    success: false,
                    error: Some("Not found".to_string()),
                    data: None,
                },
            ),
        }
    }

    async fn handle_status(&self) -> Result<Response<Body>, Infallible> {
        let StatusSnapshot {
            client_count,
            forward_token_count,
            reverse_token_count,
            connector_token_count,
        } = self.server.status_snapshot().await;

        self.json(
            StatusCode::OK,
            ApiResponse {
                success: true,
                error: None,
                data: Some(ServerStatus {
                    client_count,
                    forward_token_count,
                    reverse_token_count,
                    connector_token_count,
                }),
            },
        )
    }

    async fn handle_list_tokens(&self) -> Result<Response<Body>, Infallible> {
        let snapshots: Vec<TokenSnapshot> = self.server.token_snapshot().await;

        let tokens: Vec<TokenInfo> = snapshots
            .into_iter()
            .map(|snapshot| TokenInfo {
                token: snapshot.token,
                port: snapshot.port,
                client_count: snapshot.client_count,
            })
            .collect();

        self.json(
            StatusCode::OK,
            ApiResponse {
                success: true,
                error: None,
                data: Some(tokens),
            },
        )
    }

    async fn handle_add_token(&self, req: Request<Body>) -> Result<Response<Body>, Infallible> {
        let body = hyper::body::to_bytes(req.into_body())
            .await
            .unwrap_or_default();
        let payload: AddTokenRequest = match serde_json::from_slice(&body) {
            Ok(payload) => payload,
            Err(err) => {
                return self.json(
                    StatusCode::BAD_REQUEST,
                    ApiResponse::<()> {
                        success: false,
                        error: Some(format!("Invalid JSON payload: {}", err)),
                        data: None,
                    },
                );
            }
        };

        if payload.reverse {
            let options = ReverseTokenOptions {
                token: payload.token,
                port: payload.port,
                username: payload.username,
                password: payload.password,
                allow_manage_connector: payload.allow_manage_connector.unwrap_or(false),
            };

            match self.server.add_reverse_token(options).await {
                Ok(result) => {
                    let token_info = TokenInfo {
                        token: result.token,
                        port: result.port,
                        client_count: 0,
                    };
                    self.json(
                        StatusCode::OK,
                        ApiResponse {
                            success: true,
                            error: None,
                            data: Some(token_info),
                        },
                    )
                }
                Err(err) => self.json(
                    StatusCode::BAD_REQUEST,
                    ApiResponse::<()> {
                        success: false,
                        error: Some(err),
                        data: None,
                    },
                ),
            }
        } else {
            match self.server.add_forward_token(payload.token).await {
                Ok(token) => {
                    let token_info = TokenInfo {
                        token,
                        port: None,
                        client_count: 0,
                    };
                    self.json(
                        StatusCode::OK,
                        ApiResponse {
                            success: true,
                            error: None,
                            data: Some(token_info),
                        },
                    )
                }
                Err(err) => self.json(
                    StatusCode::BAD_REQUEST,
                    ApiResponse::<()> {
                        success: false,
                        error: Some(err),
                        data: None,
                    },
                ),
            }
        }
    }

    async fn handle_remove_token(&self, token: &str) -> Result<Response<Body>, Infallible> {
        if token.is_empty() {
            return self.json(
                StatusCode::BAD_REQUEST,
                ApiResponse::<()> {
                    success: false,
                    error: Some("Missing token parameter".to_string()),
                    data: None,
                },
            );
        }

        let removed = self.server.remove_token(token).await;
        if removed {
            self.json(
                StatusCode::OK,
                ApiResponse::<()> {
                    success: true,
                    error: None,
                    data: None,
                },
            )
        } else {
            self.json(
                StatusCode::NOT_FOUND,
                ApiResponse::<()> {
                    success: false,
                    error: Some("Token not found".to_string()),
                    data: None,
                },
            )
        }
    }

    async fn handle_add_connector(&self, req: Request<Body>) -> Result<Response<Body>, Infallible> {
        let body = hyper::body::to_bytes(req.into_body())
            .await
            .unwrap_or_default();
        let payload: AddConnectorRequest = match serde_json::from_slice(&body) {
            Ok(payload) => payload,
            Err(err) => {
                return self.json(
                    StatusCode::BAD_REQUEST,
                    ApiResponse::<()> {
                        success: false,
                        error: Some(format!("Invalid JSON payload: {}", err)),
                        data: None,
                    },
                );
            }
        };

        if payload.reverse_token.trim().is_empty() {
            return self.json(
                StatusCode::BAD_REQUEST,
                ApiResponse::<()> {
                    success: false,
                    error: Some("reverse_token must be provided".to_string()),
                    data: None,
                },
            );
        }

        match self
            .server
            .add_connector_token(payload.connector_token, &payload.reverse_token)
            .await
        {
            Ok(token) => {
                let info = TokenInfo {
                    token,
                    port: None,
                    client_count: 0,
                };
                self.json(
                    StatusCode::OK,
                    ApiResponse {
                        success: true,
                        error: None,
                        data: Some(info),
                    },
                )
            }
            Err(err) => self.json(
                StatusCode::BAD_REQUEST,
                ApiResponse::<()> {
                    success: false,
                    error: Some(err),
                    data: None,
                },
            ),
        }
    }

    async fn handle_remove_connector(&self, token: &str) -> Result<Response<Body>, Infallible> {
        if token.is_empty() {
            return self.json(
                StatusCode::BAD_REQUEST,
                ApiResponse::<()> {
                    success: false,
                    error: Some("Missing connector token parameter".to_string()),
                    data: None,
                },
            );
        }

        let removed = self.server.remove_connector_token(token).await;
        if removed {
            self.json(
                StatusCode::OK,
                ApiResponse::<()> {
                    success: true,
                    error: None,
                    data: None,
                },
            )
        } else {
            self.json(
                StatusCode::NOT_FOUND,
                ApiResponse::<()> {
                    success: false,
                    error: Some("Connector token not found".to_string()),
                    data: None,
                },
            )
        }
    }

    fn json<T: Serialize>(
        &self,
        status: StatusCode,
        payload: ApiResponse<T>,
    ) -> Result<Response<Body>, Infallible> {
        let body = serde_json::to_vec(&payload).unwrap_or_else(|_| {
            serde_json::to_vec(&ApiResponse::<()> {
                success: false,
                error: Some("Failed to serialize response".to_string()),
                data: None,
            })
            .unwrap()
        });

        Ok(Response::builder()
            .status(status)
            .header(hyper::header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap())
    }
}
