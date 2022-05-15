#!/bin/sh

sudo install target/release/simu target/release/simu_suid_helper /usr/local/bin/

sudo chown root:root /usr/local/bin/simu_suid_helper
sudo chmod u+s /usr/local/bin/simu_suid_helper

sudo cp ./sample-simu.service /etc/systemd/system/simu.service

sudo mkdir -p /usr/local/share/simu/templates
sudo cp static/templates/* /usr/local/share/simu/templates

sudo systemctl daemon-reload
