#!/bin/bash

# sudo crontab -e : sudo is needed, because certbot needs sudo
# Daily check at 06:59 UTC: (daily auto deploy happens at 6:50 UTC) if HTTPS certs need renewal, and renew if needed we restart the webserver to pick up the new certs.
# 59 6 * * * /home/sysadmin/RQ/admin/https_certs/https_certs_renew.sh >> /home/sysadmin/RQ/admin/https_certs/https_certs_renew.log 2>&1

# Configuration
DAYS_THRESHOLD=35   # Threshold for renewal (days until expiration)

# Run certbot certificates and extract days valid
echo "Checking certificate validity..."
CERT_OUTPUT=$(sudo certbot certificates 2>/dev/null)
NUM_DAYS_VALID=$(echo "$CERT_OUTPUT" | grep -oP 'VALID: \K\d+(?= days)' | head -n 1)
# The script gets the first VALID: X days value. Since certificates for rqcore.com and thetaconite.com were issued together, they should have the same expiration. 
# Later, when we add more domains, we may need to handle them separately.

# Check if NUM_DAYS_VALID was extracted
if [ -z "$NUM_DAYS_VALID" ]; then
  echo "Error: Could not extract days valid from certbot output."
  echo "Certbot output: $CERT_OUTPUT"
  exit 1
fi

# Echo the number of days valid
echo "Certificates valid for: $NUM_DAYS_VALID days"

# Check if days valid is more than threshold
if [ "$NUM_DAYS_VALID" -gt "$DAYS_THRESHOLD" ]; then
  echo "More than $DAYS_THRESHOLD days until expiration. Exiting."
  exit 0
fi

# If 35 days or less, proceed with renewal
echo "Certificates have $NUM_DAYS_VALID days left, renewing..."
sudo certbot renew --quiet

# <Maybe not!> Reload web server to apply renewed certificates
# echo "Reloading WebServer..."
# sudo systemctl reload nginx

# We renewed the certs 35 days earlier. Killing the Rust Webserver is too cruel.
# The webserver itself should check the HTTPS certs file date every day (or week) and reload them if they changed. 
# But no need to restart the whole webserver as it might do important work.

echo "Certificate renewal check complete."