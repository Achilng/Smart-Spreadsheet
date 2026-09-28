import https from 'node:https';
import dns from 'node:dns';
import { BlockList, isIP } from 'node:net';
import { Readable } from 'node:stream';

// Custom channels on the public site may reach public HTTPS services only.
// Validate the addresses in the socket's own DNS lookup, including every retry;
// a preflight DNS check followed by fetch would permit DNS rebinding.
const blocked = new BlockList();
for (const [address, prefix] of [['0.0.0.0',8],['10.0.0.0',8],['100.64.0.0',10],['127.0.0.0',8],['169.254.0.0',16],['172.16.0.0',12],['192.0.0.0',24],['192.168.0.0',16],['198.18.0.0',15],['224.0.0.0',4],['240.0.0.0',4]]) blocked.addSubnet(address, prefix, 'ipv4');
const globalV6 = new BlockList(); globalV6.addSubnet('2000::', 3, 'ipv6');
const excludedV6 = new BlockList();
for (const [address, prefix] of [['2001::',32],['2001:db8::',32],['2002::',16]]) excludedV6.addSubnet(address, prefix, 'ipv6');
export function publicAddress(address) {
  const family = isIP(address);
  return family === 4 ? !blocked.check(address, 'ipv4') : family === 6 && globalV6.check(address, 'ipv6') && !excludedV6.check(address, 'ipv6');
}
export function publicFetch(url, init = {}) {
  return new Promise((resolve, reject) => {
    const target = new URL(url), literal = target.hostname.replace(/^\[|\]$/g, '');
    if (target.protocol !== 'https:' || target.username || target.password || (isIP(literal) && !publicAddress(literal))) return reject(Error('自定义渠道只支持公网 HTTPS 地址。'));
    const request = https.request(target, { method: init.method || 'GET', headers: init.headers, signal: init.signal,
      lookup(host, options, callback) {
        dns.lookup(host, { all: true }, (error, addresses) => {
          if (error) return callback(error);
          const allowed = addresses.filter(x => publicAddress(x.address));
          if (!allowed.length) return callback(Error('自定义渠道不能访问服务器内网地址。'));
          if (options.all) callback(null, allowed);
          else callback(null, allowed[0].address, allowed[0].family);
        });
      },
    }, response => {
      if (response.statusCode >= 300 && response.statusCode < 400) { response.resume(); return reject(Error('接口返回跳转，请填写最终 Base URL。')); }
      const headers = new Headers();
      for (const [key, value] of Object.entries(response.headers)) if (value !== undefined) headers.set(key, Array.isArray(value) ? value.join(', ') : value);
      resolve(new Response([204,205,304].includes(response.statusCode) ? null : Readable.toWeb(response), { status: response.statusCode, headers }));
    });
    request.on('error', reject); request.end(init.body);
  });
}
