#!/bin/bash

# >General Note: in SH scripts, no file logging handling is really needed if the log filename is static (e.g. not calculated by Date). 
# This way, it can be run manually too, and the echos are visible if it is run manually.
# Just run any SH script this way:
#     /home/rquser/RQ/rqcoresrv/deploy.sh >> /home/rquser/RQ/rqcoresrv/deploy.log 2>&1
# It 'appends' all normal output (stdout('1')) and error messages (stderr ('2') from the script to the file /home/rquser/RQ/rqcoresrv/deploy.log.

# Deployment script for rqcoresrv
# Run from ~/RQ/rqcoresrv

# rqcoresrv deploy crontab scheduling is set to 6:50 UTC, so we expect that webserver is ready at 07:00 UTC every day
# Because at Interactive Brokers (IBKR) the daily reset official schedule is 00:15–01:45 ET.
# During EST (winter, UTC-5): it is 05:15–06:45 UTC
# During EDT (summer, UTC-4): it is 04:15–05:45 UTC
# So, we should target our reset After IBKR reset. That is at 06:50 UTC.
# Add the cron job to run the script /home/rquser/RQ/rqcoresrv/deploy.sh daily at 06:50 UTC:
# 50 6 * * * /bin/bash /home/rquser/RQ/rqcoresrv/deploy.sh >> /home/rquser/RQ/rqcoresrv/deploy.log 2>&1

ROOT=~/RQ/rqcoresrv

echo -e "\n*** Deployment started at $(date)"

# 1. Git Pull to staging
cd $ROOT/staging
echo "*** Pulling latest from GitHub to staging..."
git pull 2>&1

STAGING_COMMIT=$(git rev-parse HEAD)
cd $ROOT/prod
PROD_COMMIT=$(git rev-parse HEAD)

echo "*** Staging commit: $STAGING_COMMIT"
echo "*** Production commit: $PROD_COMMIT"

if [ "$STAGING_COMMIT" == "$PROD_COMMIT" ]; then
    echo "*** No changes detected. No need for new deployment."
    exit 0
fi

cd $ROOT

# 2. Compile and test
echo "*** Compiling in release mode..."
cd $ROOT/staging/src/rqcoresrv
cargo build --release 2>&1

echo "*** Running tests..."
#cd ../rqcoresrv_test
#cargo test 2>&1
#if [ $? -ne 0 ]; then
    #echo "*** Tests failed. Exiting deployment."
#    exit 1
#fi

cd $ROOT

# 3. Kill old app, copy folder, start new app
echo "*** Killing existing screen session 'rqcoresrv' if it exists..."
screen -ls 2>&1 | grep '(Detached)' | grep -o 'rqcoresrv' | xargs -I{} -n 1 -r screen -r -S {} -X quit 2>&1

sleep 1

CURRENT_DATE=$(date +%Y%m%d)
echo "*** Renaming old prod to prod_$CURRENT_DATE..."
if [ -d prod_$CURRENT_DATE ]; then
    rm -rf prod_$CURRENT_DATE
fi
mv prod prod_$CURRENT_DATE

echo "*** Creating new prod folder..."
mkdir prod

echo "*** Copying staging to prod..."
cp -r staging/. prod/

echo "*** Starting new screen session 'rqcoresrv'..."
screen -S "rqcoresrv" -d -m
echo "*** A new screen 'rqcoresrv' is created. Sleeping for 1 sec before sending command to start webserver..."
sleep 1

screen -r "rqcoresrv" -X stuff $'cd /home/rquser/RQ/rqcoresrv/prod/src/rqcoresrv\n./target/release/rqcoresrv\n'

echo "*** Deployment completed at $(date)"