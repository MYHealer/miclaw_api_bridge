'use strict';
'require view';
'require uci';
'require rpc';
'require ui';

/*
 * Mimo Bridge — client-side LuCI view (openwrt-24.10 / JS client).
 *
 * Renders the bridge's own WebUI inside an iframe. The bridge serves a full
 * SPA (login / dashboard / logs) at http://<router>:<port>/ with same-origin
 * /api and /v1 calls, so we just point an iframe at it. No proxying needed.
 */

var callServiceList = rpc.declare({
	object: 'service',
	method: 'list',
	params: ['name'],
	expect: { '': {} }
});

function bridgePort() {
	var p = uci.get('miclaw_api_bridge', 'main', 'port');
	p = parseInt(p, 10);
	return (p > 0 && p < 65536) ? p : 8765;
}

function bridgeRunning(serviceState) {
	try {
		var inst = serviceState['miclaw_api_bridge']['instances'];
		for (var k in inst)
			if (inst[k].running) return true;
	} catch (e) {}
	return false;
}

return view.extend({
	load: function() {
		return Promise.all([
			uci.load('miclaw_api_bridge').catch(function() { return null; }),
			callServiceList('miclaw_api_bridge').catch(function() { return {}; })
		]);
	},

	render: function(data) {
		var serviceState = data[1] || {};
		var running = bridgeRunning(serviceState);
		var port = bridgePort();

		var host = window.location.hostname;
		var proto = window.location.protocol; // 'http:' | 'https:'
		var url = 'http://' + host + ':' + port + '/';

		var nodes = [
			E('h2', {}, _('Mimo Bridge')),
			E('div', { 'class': 'cbi-map-descr' },
				_('Local OpenAI / Anthropic-compatible endpoint backed by Xiaomi mimo. ' +
				  'The panel below is the bridge\'s own WebUI served at port %d.').format(port))
		];

		/* Service status line */
		nodes.push(E('div', { 'class': 'cbi-section', 'style': 'margin-bottom:.5em' }, [
			E('span', { 'style': 'margin-right:1em' }, [
				E('strong', {}, _('Service') + ': '),
				running
					? E('span', { 'style': 'color:#2e7d32' }, _('running'))
					: E('span', { 'style': 'color:#c62828' }, _('not running'))
			]),
			E('a', {
				'href': url,
				'target': '_blank',
				'rel': 'noopener',
				'class': 'btn cbi-button'
			}, _('Open in new tab')),
			' ',
			E('span', { 'style': 'opacity:.7' }, url)
		]));

		/* Mixed-content guard: HTTPS LuCI cannot iframe a plain-HTTP bridge. */
		if (proto === 'https:') {
			nodes.push(E('div', { 'class': 'alert-message warning' }, [
				E('p', {}, _('LuCI is being served over HTTPS, but the bridge speaks plain HTTP. ' +
					'Browsers block embedding HTTP content inside an HTTPS page, so the panel ' +
					'below may stay blank. Either open LuCI over http://%s/ or use the ' +
					'"Open in new tab" button above.').format(host))
			]));
		}

		if (!running) {
			nodes.push(E('div', { 'class': 'alert-message warning' }, [
				E('p', {}, _('The miclaw_api_bridge service is not running. Start it with ' +
					'"/etc/init.d/miclaw_api_bridge start" or enable it on boot.'))
			]));
		}

		/* The embedded WebUI */
		var iframe = E('iframe', {
			'src': url,
			'id': 'miclaw-frame',
			'style': 'width:100%; height:calc(100vh - 230px); min-height:480px; ' +
				'border:1px solid #ccc; border-radius:6px; background:#fff;',
			'frameborder': '0',
			'allow': 'clipboard-write'
		});

		nodes.push(E('div', { 'class': 'cbi-section' }, [ iframe ]));

		return E('div', { 'class': 'cbi-map' }, nodes);
	},

	/* No form to save on this page. */
	handleSaveApply: null,
	handleSave: null,
	handleReset: null
});
