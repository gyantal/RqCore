#!/bin/bash

# See the KEY version: GDriveHedgeQuant\shared\RqCoreServer\Reinstall\DNS\dynamicip_dnsupdate.sh

# There is no reason why this cannot be run by the standard rquser, sysadmin is not needed. It is a DNS update on the DNS servers.
# However, we use sysadmin user, because:
# - Allowing a non-admin user to manage certificates increases the risk of misconfiguration or exposure of private keys (e.g Clouflare API Token) if the non-admin user account is hacked.
# - This is the standard approach for utils that expose passwords and aligns with best practices. (well tested)

# Add this to user's crontab (no sudo needed):
# Dynamic IP behind router can change. DNS update with Cloudflare API. This runs the script at 0:00, 6:00, 12:00, and 18:00 daily.
# 0 */6 * * * /home/sysadmin/RQ/admin/dnsupdate/dynamic_ip_dnsupdate.sh

# DNS propagation checker: https://www.whatsmydns.net/#A/rqcore.com
# Subdomains: If you need to update subdomains (e.g., www.rqcore.com), adjust the name field in the API call accordingly.

# Configuration
API_KEY="<FILL>"
EMAIL="<FILL>@gmail.com"

DOMAINS=("rqcore.com" "thetaconite.com")

ZONE_ID_RQCORE="24f9cd7f24067282f781cb1e0700031a"  # Zone ID for rqcore.com
RECORD_ID_RQCORE="0a9aaa84419a4bb69ffb980481d2d182"  # DNS Record ID for rqcore.com A record

ZONE_ID_THETACONITE="689f1e93dec401d9a1a03e68e1acc3c4"  # Zone ID for thetaconite.com
RECORD_ID_THETACONITE="6e55bb333866c08bbb4266cc27740a2f"  # DNS Record ID for thetaconite.com A record

LOG_FILE="/home/sysadmin/RQ/admin/dnsupdate/dynamic_ip_dnsupdate.log"

ZONE_IDS=("$ZONE_ID_RQCORE" "$ZONE_ID_THETACONITE")
RECORD_IDS=("$RECORD_ID_RQCORE" "$RECORD_ID_THETACONITE")

# Function to log messages
log_message() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') - $1" >> "$LOG_FILE"
}

# Start logging
log_message "***Starting DNS update script"

# Get current WAN IP
WAN_IP=$(curl -s https://api.ipify.org)
if [ -z "$WAN_IP" ]; then
    log_message "ERROR: Failed to retrieve WAN IP"
    exit 1
fi
log_message "Current WAN IP: $WAN_IP"

# Update DNS records for each domain
for i in "${!DOMAINS[@]}"; do
    DOMAIN=${DOMAINS[$i]}
    ZONE_ID=${ZONE_IDS[$i]}
    RECORD_ID=${RECORD_IDS[$i]}

    # Check current DNS record
    CURRENT_IP=$(curl -s -X GET "https://api.cloudflare.com/client/v4/zones/$ZONE_ID/dns_records/$RECORD_ID" \
        -H "X-Auth-Email: $EMAIL" \
        -H "X-Auth-Key: $API_KEY" \
        -H "Content-Type: application/json" | jq -r '.result.content')

    if [ "$CURRENT_IP" == "$WAN_IP" ]; then
        log_message "$DOMAIN: IP unchanged ($WAN_IP), no update needed"
        continue
    fi

    # Update DNS record
    RESPONSE=$(curl -s -X PUT "https://api.cloudflare.com/client/v4/zones/$ZONE_ID/dns_records/$RECORD_ID" \
        -H "X-Auth-Email: $EMAIL" \
        -H "X-Auth-Key: $API_KEY" \
        -H "Content-Type: application/json" \
        --data "{\"type\":\"A\",\"name\":\"$DOMAIN\",\"content\":\"$WAN_IP\",\"ttl\":120,\"proxied\":false}")

    # Check if update was successful
    if echo "$RESPONSE" | jq -r '.success' | grep -q "true"; then
        log_message "$DOMAIN: Successfully updated to IP $WAN_IP"
    else
        log_message "$DOMAIN: ERROR - Failed to update IP. Response: $RESPONSE"
    fi
done

# End logging
log_message "DNS update script completed"