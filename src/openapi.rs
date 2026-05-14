use axum::{
    response::{Html, Redirect},
    Json,
};
use serde_json::{json, Value};

const SWAGGER_UI_HTML: &str = r##"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>TAP App Attest Server API</title>
    <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5/swagger-ui.css" />
    <style>
      body { margin: 0; background: #f7f7f7; }
      .swagger-ui .topbar { display: none; }
    </style>
  </head>
  <body>
    <div id="swagger-ui"></div>
    <script src="https://cdn.jsdelivr.net/npm/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
    <script>
      window.ui = SwaggerUIBundle({
        url: "/openapi.json",
        dom_id: "#swagger-ui",
        deepLinking: true,
        persistAuthorization: false,
        displayRequestDuration: true
      });
    </script>
  </body>
</html>
"##;

pub async fn swagger_ui() -> Html<&'static str> {
    Html(SWAGGER_UI_HTML)
}

pub async fn docs_redirect() -> Redirect {
    Redirect::temporary("/swagger-ui")
}

pub async fn openapi_json() -> Json<Value> {
    Json(spec())
}

fn spec() -> Value {
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "TAP App Attest Server API",
            "version": "0.1.0",
            "description": "Backend endpoints for TAPCamDemo App Attest attestation registration. Assertion verification is planned but not implemented in this version."
        },
        "servers": [
            {
                "url": "/",
                "description": "Current host"
            }
        ],
        "tags": [
            {
                "name": "Health",
                "description": "Process and Redis health checks"
            },
            {
                "name": "App Attest",
                "description": "Challenge issuance, attestation registration, and credential status"
            }
        ],
        "paths": {
            "/healthz": {
                "get": {
                    "tags": ["Health"],
                    "operationId": "healthz",
                    "summary": "Check service and Redis health",
                    "responses": {
                        "200": {
                            "description": "The service and Redis are reachable.",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/HealthResponse" },
                                    "example": { "status": "ok", "redis": "ok" }
                                }
                            }
                        },
                        "503": { "$ref": "#/components/responses/ErrorResponse" }
                    }
                }
            },
            "/app-attest/challenges": {
                "post": {
                    "tags": ["App Attest"],
                    "operationId": "createChallenge",
                    "summary": "Issue an App Attest challenge",
                    "description": "Creates a 32-byte CSPRNG challenge, stores it in Redis with a one-hour default TTL, and returns it as base64url without padding.",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/ChallengeRequest" },
                                "example": {
                                    "purpose": "attestation",
                                    "credentialName": "photo_keyid"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Challenge issued.",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ChallengeResponse" },
                                    "example": {
                                        "challengeId": "9f7e3a97-6b91-44fe-94f4-6b7d65f70c53",
                                        "challenge": "l6dQz3cI1fR46XW1pVZ_p9BImRPgRYA3yVfA-08Om3E",
                                        "expiresAt": "2026-05-14T11:30:00Z"
                                    }
                                }
                            }
                        },
                        "400": { "$ref": "#/components/responses/ErrorResponse" },
                        "503": { "$ref": "#/components/responses/ErrorResponse" }
                    }
                }
            },
            "/app-attest/attestations": {
                "post": {
                    "tags": ["App Attest"],
                    "operationId": "registerAttestation",
                    "summary": "Verify and register an App Attest attestation",
                    "description": "Consumes an attestation challenge, verifies the Apple App Attest attestation object, checks that the request keyId matches the attested credential id, then stores the credential in Redis.",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/AttestationRequest" },
                                "example": {
                                    "credentialName": "photo_keyid",
                                    "keyId": "base64urlAppleKeyId",
                                    "challengeId": "9f7e3a97-6b91-44fe-94f4-6b7d65f70c53",
                                    "attestationObject": "base64urlCborAttestationObject"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Attestation accepted and credential stored.",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/AttestationResponse" },
                                    "example": {
                                        "credentialId": "base64urlCredentialId",
                                        "status": "accepted"
                                    }
                                }
                            }
                        },
                        "400": { "$ref": "#/components/responses/ErrorResponse" },
                        "404": { "$ref": "#/components/responses/ErrorResponse" },
                        "503": { "$ref": "#/components/responses/ErrorResponse" }
                    }
                }
            },
            "/app-attest/credentials/status": {
                "post": {
                    "tags": ["App Attest"],
                    "operationId": "credentialStatus",
                    "summary": "Check credential status",
                    "description": "Returns the AppAttestKit-compatible status string for a credential.",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": { "$ref": "#/components/schemas/CredentialStatusRequest" },
                                "examples": {
                                    "withKeyId": {
                                        "value": {
                                            "credentialName": "photo_keyid",
                                            "keyId": "base64urlAppleKeyId"
                                        }
                                    },
                                    "withoutKeyId": {
                                        "value": {
                                            "credentialName": "photo_keyid",
                                            "keyId": null
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Credential status.",
                            "content": {
                                "application/json": {
                                    "schema": { "$ref": "#/components/schemas/ServerCredentialStatus" },
                                    "example": "accepted"
                                }
                            }
                        },
                        "400": { "$ref": "#/components/responses/ErrorResponse" },
                        "503": { "$ref": "#/components/responses/ErrorResponse" }
                    }
                }
            }
        },
        "components": {
            "responses": {
                "ErrorResponse": {
                    "description": "Structured error response.",
                    "content": {
                        "application/json": {
                            "schema": { "$ref": "#/components/schemas/ErrorEnvelope" }
                        }
                    }
                }
            },
            "schemas": {
                "HealthResponse": {
                    "type": "object",
                    "required": ["status", "redis"],
                    "properties": {
                        "status": { "type": "string", "example": "ok" },
                        "redis": { "type": "string", "example": "ok" }
                    }
                },
                "AppAttestPurpose": {
                    "type": "string",
                    "enum": ["attestation", "assertion"]
                },
                "ChallengeRequest": {
                    "type": "object",
                    "required": ["purpose", "credentialName"],
                    "properties": {
                        "purpose": { "$ref": "#/components/schemas/AppAttestPurpose" },
                        "credentialName": {
                            "type": "string",
                            "minLength": 1,
                            "example": "photo_keyid"
                        }
                    }
                },
                "ChallengeResponse": {
                    "type": "object",
                    "required": ["challengeId", "challenge", "expiresAt"],
                    "properties": {
                        "challengeId": {
                            "type": "string",
                            "format": "uuid"
                        },
                        "challenge": {
                            "type": "string",
                            "description": "Base64url-no-padding raw 32-byte challenge."
                        },
                        "expiresAt": {
                            "type": "string",
                            "format": "date-time"
                        }
                    }
                },
                "AttestationRequest": {
                    "type": "object",
                    "required": ["credentialName", "keyId", "challengeId", "attestationObject"],
                    "properties": {
                        "credentialName": {
                            "type": "string",
                            "minLength": 1,
                            "example": "photo_keyid"
                        },
                        "keyId": {
                            "type": "string",
                            "minLength": 1,
                            "description": "App Attest key id from the client. It must match the credential id inside the attestation object."
                        },
                        "challengeId": {
                            "type": "string",
                            "minLength": 1
                        },
                        "attestationObject": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Base64url encoded CBOR attestation object."
                        }
                    }
                },
                "RegistrationStatus": {
                    "type": "string",
                    "enum": ["accepted"]
                },
                "AttestationResponse": {
                    "type": "object",
                    "required": ["status"],
                    "properties": {
                        "credentialId": {
                            "type": "string",
                            "nullable": true
                        },
                        "status": { "$ref": "#/components/schemas/RegistrationStatus" }
                    }
                },
                "CredentialStatusRequest": {
                    "type": "object",
                    "required": ["credentialName"],
                    "properties": {
                        "credentialName": {
                            "type": "string",
                            "minLength": 1,
                            "example": "photo_keyid"
                        },
                        "keyId": {
                            "type": "string",
                            "nullable": true
                        }
                    }
                },
                "ServerCredentialStatus": {
                    "type": "string",
                    "enum": ["accepted", "revoked", "unknown"]
                },
                "ErrorEnvelope": {
                    "type": "object",
                    "required": ["error"],
                    "properties": {
                        "error": { "$ref": "#/components/schemas/ErrorBody" }
                    }
                },
                "ErrorBody": {
                    "type": "object",
                    "required": ["code", "message", "details"],
                    "properties": {
                        "code": {
                            "type": "string",
                            "example": "CHALLENGE_INVALID"
                        },
                        "message": {
                            "type": "string"
                        },
                        "details": {
                            "type": "object"
                        },
                        "upstream": {
                            "$ref": "#/components/schemas/UpstreamError",
                            "nullable": true
                        }
                    }
                },
                "UpstreamError": {
                    "type": "object",
                    "required": ["validationStage", "errorCode", "message"],
                    "properties": {
                        "validationStage": { "type": "string" },
                        "errorCode": { "type": "string" },
                        "message": { "type": "string" }
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_spec_lists_public_endpoints() {
        let spec = spec();
        let paths = spec["paths"].as_object().unwrap();
        assert!(paths.contains_key("/healthz"));
        assert!(paths.contains_key("/app-attest/challenges"));
        assert!(paths.contains_key("/app-attest/attestations"));
        assert!(paths.contains_key("/app-attest/credentials/status"));
    }

    #[test]
    fn swagger_ui_loads_local_openapi_json() {
        assert!(SWAGGER_UI_HTML.contains("SwaggerUIBundle"));
        assert!(SWAGGER_UI_HTML.contains("/openapi.json"));
    }
}
