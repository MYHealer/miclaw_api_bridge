# OpenWrt SDK package Makefile for the LuCI Mimo Bridge app — OPTIONAL.
#
# NOTE: You usually do NOT need this. `openwrt/build-ipk.sh` already produces a
# working, opkg-installable .ipk WITHOUT the SDK (verified on OpenWrt 24.10).
# This Makefile only matters if you want the package built inside an official
# OpenWrt SDK / buildroot (e.g. to publish it in a custom feed).
#
# Build inside an OpenWrt SDK matching your firmware
# (mediatek/filogic, ImmortalWrt 24.10, arch aarch64_cortex-a53):
#
#   1. Copy this folder to <sdk>/package/luci-app-miclaw
#   2. Copy the LuCI sources next to it:
#        cp -r ../../luci/htdocs ../../luci/root  <sdk>/package/luci-app-miclaw/
#   3. make package/luci-app-miclaw/compile V=s
#
# This packages ONLY the LuCI frontend. The Rust server binary + init script +
# UCI config are deployed separately (see openwrt/README.md), because the binary
# is cross-compiled with cargo, not the SDK.

include $(TOPDIR)/rules.mk

LUCI_TITLE:=LuCI support for miclaw_api_bridge (Mimo Bridge)
LUCI_DEPENDS:=
LUCI_PKGARCH:=all

PKG_NAME:=luci-app-miclaw
PKG_VERSION:=0.1.0
PKG_RELEASE:=1
PKG_LICENSE:=MIT
PKG_MAINTAINER:=neoruaa

include $(TOPDIR)/feeds/luci/luci.mk

# call BuildPackage - OpenWrt buildroot signature
