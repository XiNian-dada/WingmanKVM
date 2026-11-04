# 会话 Cookie 与 `server.public_origin`

WingmanKVM 用 `wingman_session` Cookie 保存网页管理会话。Cookie 的 `Secure`
属性由内置 TLS 状态及 `server.public_origin` 决定，不由任何请求头决定。内置 TLS 启用时始终带 `Secure`。

## 策略

新安装默认启用内置 TLS，首次设置、登录和退出响应均使用 Secure Cookie。下表仅适用于 `server.tls.mode = "disabled"` 的 HTTP 监听（包括 TLS 由反向代理终止的场景）：

| `server.public_origin` | 新会话 Cookie（首次设置、密码登录、Passkey 登录） | 退出登录删除 Cookie |
| --- | --- | --- |
| 未配置 | 省略 `Secure`，保留直接 HTTP 调试兼容性 | 省略 `Secure` |
| `http://...` | 省略 `Secure`，保留直接 HTTP 调试兼容性 | 省略 `Secure` |
| `https://...` | 始终带 `Secure` | 始终带 `Secure` |
| 无效的 `Some` 值（仅内存异常） | 故障安全，始终带 `Secure` | 始终带 `Secure` |

Cookie 名称、`Path=/`、`HttpOnly`、`SameSite=Strict`、有效期和认证流程保持不变。
新会话的 `Max-Age` 为 43200；退出登录使用相同的属性并以 `Max-Age=0` 清除。

服务在生成每个响应时读取当前已验证的 TLS 配置及 `server.public_origin`。
如果内存中的值无法解析，按故障安全策略仍带 `Secure`；正常加载和保存配置时会拒绝
无效的 origin。

## 在首次设置前配置

内置 TLS 默认无需预先配置即可签发 Secure Cookie。若由反向代理终止 HTTPS，
`public_origin` 应在创建管理员之前写入状态目录中的 `config.json`，因为首次设置
响应本身就会签发会话 Cookie。例如：

```json
{
  "version": 1,
  "server": {
    "listen_address": "0.0.0.0",
    "port": 8080,
    "tls": { "mode": "disabled" },
    "public_origin": "https://kvm.example.com"
  }
}
```

该值只能是严格的 HTTP(S) origin，例如 `https://kvm.example.com`；不能包含用户信息、
查询、片段或非根路径。设置完成后，服务按当前配置为后续登录和退出响应生成 Cookie。

## 直接 HTTP 调试

在主机配置 `server.tls.mode = "disabled"`，同时不配置 `public_origin`，或显式配置 `http://...`，会保留直接通过 HTTP 访问的调试
兼容性：Cookie 不带 `Secure`，浏览器可以在 HTTP 页面保存会话。该模式不应作为公网
部署方案。

## 从 HTTP 部署切换到 HTTPS

在 **设置 → HTTPS 与证书** 启用自签模式时，服务会撤销旧会话，并在保存响应中清除当前浏览器的旧非 Secure Cookie。打开新的 HTTPS 地址后重新登录，新 Cookie 带 `Secure`。其他浏览器的旧 Cookie 即使仍存在也不再是有效会话。

仅编辑磁盘配置不会修改客户端 Cookie；重启会清空服务端会话，应在浏览器清除旧 Cookie 后通过 HTTPS 重新登录。内置 TLS 的同端口 HTTP 请求只用于 GET/HEAD 跳转和本机健康检查，不再接受明文登录或控制请求。HTTP 请求到达服务器之前可能已经携带旧 Cookie，重定向不能把这次传输变成加密传输。

## 不信任代理请求头

`Host`、`Origin`、`X-Forwarded-Proto`、`X-Forwarded-Ssl`、`Forwarded` 等请求头
不会改变 Cookie 的 `Secure` 属性。因此：

- HTTP 调试模式下，伪造 `X-Forwarded-Proto: https` 不会让 Cookie 变成 Secure；
- 配置了 HTTPS `public_origin` 时，伪造 `X-Forwarded-Proto: http` 或其他降级头
  不会移除 `Secure`。

反向代理必须自行正确传递请求，但不能把 Cookie 策略建立在客户端可伪造的头上。

## 退出登录的一致性

退出登录通过同一个格式化函数生成删除 Cookie，名称、`Path`、`HttpOnly`、
`SameSite=Strict` 和 `Secure` 与新会话 Cookie 一致，仅值清空且 `Max-Age=0`。
这样浏览器才能可靠匹配并删除已有会话 Cookie。

## TLS 监听与代理

`server.tls.mode` 为 `self_signed` 或 `custom` 时，WingmanKVM 在当前监听端口终止 TLS。仅设置 HTTPS `public_origin` 不会启用 TLS：`mode = "disabled"` 时仍是 HTTP 上游，需由代理终止 TLS。详见 [HTTPS 与证书](HTTPS.md)。

## 验证

仓库提供只会输出 Cookie 属性布尔值的黑盒脚本，实际启动隔离二进制并执行首次设置、
密码登录和退出登录：

```bash
python3 scripts/verify_p2c_cookie.py \
  --binary /path/to/wingmankvm \
  --scratch-dir /tmp/wingmankvm-p2c \
  --report /tmp/wingmankvm-p2c/report.json
```

脚本分别覆盖未配置 `public_origin` 的 HTTP 调试模式和 `https://...` 模式，并在请求中
伪造转发头；报告不包含令牌、密码或 Cookie 值。浏览器实际传输行为仍需在目标环境中
另行验证。
