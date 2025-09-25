#!/bin/bash

# Ensure Certbot and the Certbot Cloudflare plugin are installed
# sudo apt update
# sudo apt install certbot python3-certbot-dns-cloudflare

# 1. Python cannot be avoided easily: Certbot itself is written in Python, and its plugins, including python3-certbot-dns-cloudflare, are Python-based
# 2. Use sysadmin user, because:
# - Allowing a non-admin user to manage certificates increases the risk of misconfiguration or exposure of private keys (e.g Clouflare API Token) if the non-admin user account is hacked.
# - This is the standard approach for Certbot on Ubuntu and aligns with best practices. (well tested)
# 3. The result keys are here:
# server_name rqcore.com www.rqcore.com;
#     ssl_certificate /etc/letsencrypt/live/rqcore.com/fullchain.pem;
#     ssl_certificate_key /etc/letsencrypt/live/rqcore.com/privkey.pem;
# server_name thetaconite.com www.thetaconite.com;
#     ssl_certificate /etc/letsencrypt/live/thetaconite.com/fullchain.pem;
#     ssl_certificate_key /etc/letsencrypt/live/thetaconite.com/privkey.pem;
# 4. check the certificates: sudo certbot certificates

# Configuration
CLOUDFLARE_API_TOKEN="<FILL>"
DOMAINS="rqcore.com thetaconite.com"
CERTBOT_DIR="/etc/letsencrypt"
CLOUDFLARE_INI="/home/sysadmin/RQ/admin/https_certs/.cloudflare.ini"

# Create Cloudflare credentials file (supports token or key)
echo "Creating Cloudflare credentials file at $CLOUDFLARE_INI"
sudo mkdir -p "$(dirname "$CLOUDFLARE_INI")"
sudo bash -c "cat > $CLOUDFLARE_INI << EOL
dns_cloudflare_api_token = $CLOUDFLARE_API_TOKEN
EOL"
sudo chmod 600 "$CLOUDFLARE_INI"

# Request certificates for each domain
for DOMAIN in $DOMAINS; do
  echo "Requesting certificate for $DOMAIN..."
  sudo certbot certonly \
    --dns-cloudflare \
    --dns-cloudflare-credentials "$CLOUDFLARE_INI" \
    --dns-cloudflare-propagation-seconds 60 \
    --non-interactive \
    --agree-tos \
    --email "$CLOUDFLARE_EMAIL" \
    -d "$DOMAIN" \
    -d "www.$DOMAIN" \
    --debug  # Adds verbose logging; remove after testing
  if [ $? -eq 0 ]; then
    echo "Certificate for $DOMAIN created successfully."
  else
    echo "Failed to create certificate for $DOMAIN. Check /var/log/letsencrypt/letsencrypt.log"
    exit 1
  fi
done

echo "Certificates created and stored in $CERTBOT_DIR"