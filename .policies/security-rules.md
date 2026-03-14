# Policy: Security Rules

## Objective

Protect the repository and its assets from unauthorized or malicious changes.

## Rules

1. **Access Control**: Never modify authentication or authorization logic without explicit instruction and secondary review.
2. **Sensitive Data**: Do not commit secrets, private keys, or sensitive configuration to the repository.
3. **Canister Security**: Adhere to the Internet Computer best practices for canister security (e.g., proper principal validation).
4. **Boundary Protection**: Respect restricted areas defined in `.boundaries/`.
