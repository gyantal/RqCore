#!/bin/bash

# sudo crontab -e : sudo is needed, because certbot needs sudo
# Daily check at 06:59 UTC: (daily auto deploy happens at 6:50 UTC) if HTTPS certs need renewal, and renew if needed we restart the webserver to pick up the new certs.
# 59 6 * * * /home/sysadmin/RQ/admin/https_certs/https_certs_renew.sh >> /home/sysadmin/RQ/admin/https_certs/https_certs_renew.log 2>&1

# Configuration
DAYS_THRESHOLD=35   # Threshold for renewal (days until expiration)

# Run certbot certificates and extract days valid
echo "*** $(date '+%Y-%m-%d %H:%M:%S') START: Checking certificate validity..."
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
echo "Certificates have $NUM_DAYS_VALID days left, renewing with 'certbot renew --quiet'..."
sudo certbot renew --quiet

echo "After renewal attempt, checking updated certificate validity..."
CERT_OUTPUT_AFTER=$(sudo certbot certificates 2>/dev/null)
NUM_DAYS_VALID_AFTER=$(echo "$CERT_OUTPUT_AFTER" | grep -oP 'VALID: \K\d+(?= days)' | head -n 1)
echo "Certificates valid for: $NUM_DAYS_VALID_AFTER days after renewal attempt. Start copying new cert files..."

# After cert renew, copy the new certificate files to a directory owned by rquser
# chmod: give only rquser Read/Write (4+2 = 6) access.
# rqcore.com
sudo cp /etc/letsencrypt/live/rqcore.com/fullchain.pem /home/rquser/RQ/sensitive_data/https_certs/rqcore.com/
sudo cp /etc/letsencrypt/live/rqcore.com/privkey.pem /home/rquser/RQ/sensitive_data/https_certs/rqcore.com/
sudo chown rquser:rquser /home/rquser/RQ/sensitive_data/https_certs/rqcore.com/fullchain.pem
sudo chown rquser:rquser /home/rquser/RQ/sensitive_data/https_certs/rqcore.com/privkey.pem
sudo chmod 600 /home/rquser/RQ/sensitive_data/https_certs/rqcore.com/fullchain.pem
sudo chmod 600 /home/rquser/RQ/sensitive_data/https_certs/rqcore.com/privkey.pem

# thetaconite.com
sudo cp /etc/letsencrypt/live/thetaconite.com/fullchain.pem /home/rquser/RQ/sensitive_data/https_certs/thetaconite.com/
sudo cp /etc/letsencrypt/live/thetaconite.com/privkey.pem /home/rquser/RQ/sensitive_data/https_certs/thetaconite.com/
sudo chown rquser:rquser /home/rquser/RQ/sensitive_data/https_certs/thetaconite.com/fullchain.pem
sudo chown rquser:rquser /home/rquser/RQ/sensitive_data/https_certs/thetaconite.com/privkey.pem
sudo chmod 600 /home/rquser/RQ/sensitive_data/https_certs/thetaconite.com/fullchain.pem
sudo chmod 600 /home/rquser/RQ/sensitive_data/https_certs/thetaconite.com/privkey.pem

# <Maybe not!> Reload web server to apply renewed certificates
# echo "Reloading WebServer..."
# sudo systemctl reload nginx

# We renewed the certs 35 days earlier. Killing the Rust Webserver is too cruel.
# The webserver itself should check the HTTPS certs file date every day (or week) and reload them if they changed.
# But no need to restart the whole webserver as it might do important work.

echo "$(date '+%Y-%m-%d %H:%M:%S') END: Certificate renewal check complete."