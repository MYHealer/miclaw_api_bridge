#!/bin/sh
# On-router uninstaller for the Mimo Bridge OpenWrt bundle.
#   sh uninstall.sh          # remove app, keep config + session
#   sh uninstall.sh --purge  # also remove /etc/config + /etc/miclaw_api_bridge
set -e

PURGE=0
[ "${1:-}" = "--purge" ] && PURGE=1

echo "==> stopping + disabling service"
[ -x /etc/init.d/miclaw_api_bridge ] && {
	/etc/init.d/miclaw_api_bridge stop 2>/dev/null || true
	/etc/init.d/miclaw_api_bridge disable 2>/dev/null || true
}

echo "==> removing binary + init + LuCI app"
rm -f /usr/bin/miclaw_api_bridge
rm -f /etc/init.d/miclaw_api_bridge
rm -f /www/luci-static/resources/view/miclaw/overview.js
rmdir /www/luci-static/resources/view/miclaw 2>/dev/null || true
rm -f /usr/share/luci/menu.d/luci-app-miclaw.json
rm -f /usr/share/rpcd/acl.d/luci-app-miclaw.json

if [ "$PURGE" = "1" ]; then
	echo "==> purging config + session data"
	rm -f /etc/config/miclaw_api_bridge
	rm -rf /etc/miclaw_api_bridge
else
	echo "==> keeping /etc/config/miclaw_api_bridge and /etc/miclaw_api_bridge (use --purge to remove)"
fi

echo "==> refreshing LuCI / rpcd caches"
rm -f /tmp/luci-indexcache 2>/dev/null || true
rm -rf /tmp/luci-modulecache 2>/dev/null || true
/etc/init.d/rpcd reload 2>/dev/null || true
/etc/init.d/uhttpd restart 2>/dev/null || true

echo "==> done."
