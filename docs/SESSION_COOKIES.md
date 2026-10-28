# 会话 Cookie 与 `server.public_origin`

WingmanKVM 用 `wingman_session` Cookie 保存网页管理会话。Cookie 的 `Secure`
属性只由配置项 `server.public_origin` 决定，不由任何请求头决定。

## 策略

| `server.public_origin` | 新会话 Cookie（首次设置、密码登录、Passkey 登录） | 退出登录删除 Cookie |
| --- | --- | --- |
| 未配置 | 省略 `Secure`，保留直接 HTTP 调试兼容性 | 省略 `Secure` |
| `http://...` | 省略 `Secure`，保留直接 HTTP 调试兼容性 | 省略 `Secure` |
| `https://...` | 始终带 `Secure` | 始终带 `Secure` |
| 无效的 `Some` 值（仅内存异常） | 故障安全，始终带 `Secure` | 始终带 `Secure` |

Cookie 名称、`Path=/`、`HttpOnly`、`SameSite=Strict`、有效期和认证流程保持不变。
新会话的 `Max-Age` 为 43200；退出登录使用相同的属性并以 `Max-Age=0` 清除。

服务在生成每个响应时读取当前已验证的 `server.public_origin`，不使用全局可变状态。
如果内存中的值无法解析，按故障安全策略仍带 `Secure`；正常加载和保存配置时会拒绝
无效的 origin。

## 在首次设置前配置

`public_origin` 必须在创建管理员之前写入状态目录中的 `config.json`，因为首次设置
响应本身就会签发会话 Cookie。例如：

```json
{
  "version": 1,
  "server": {
    "listen_address": "0.0.0.0",
    "port": 8080,
    "public_origin": "https://kvm.example.com"
  }
}
```

该值只能是严格的 HTTP(S) origin，例如 `https://kvm.example.com`；不能包含用户信息、
查询、片段或非根路径。设置完成后，服务按当前配置为后续登录和退出响应生成 Cookie。

## 直接 HTTP 调试

不配置 `public_origin`，或显式配置 `http://...`，会保留直接通过 HTTP 访问的调试
兼容性：Cookie 不带 `Secure`，浏览器可以在 HTTP 页面保存会话。该模式不应作为公网
部署方案。

## 从 HTTP 部署切换到 HTTPS

把已运行的 HTTP 部署改为 HTTPS 时，修改 `public_origin` 或在前面增加 TLS 代理不会
追溯修改浏览器中已经保存的非 Secure 会话 Cookie。旧 Cookie 仍可能保留在原域名下并
继续被发送；只有之后新签发的 Cookie 才会带 `Secure`。

在把实例暴露到 HTTPS 或公网之前，必须：

1. 先撤销已有会话（要求管理员退出登录，或重启服务以清空内存会话）；
2. 确认旧的非 Secure 会话 Cookie 已从浏览器删除；
3. 再设置 `public_origin` 为 `https://...`，并在 HTTPS 入口重新登录；
4. 关闭 HTTP 入口，确保外部只能经 HTTPS 访问，避免旧 Cookie 继续经明文 HTTP 传输。

在完成退出/重新登录（或撤销全部会话/重启）并关闭 HTTP 入口之前，不要把该实例暴露
到公网。

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

## 不宣称 TLS 终止

设置 `https://public_origin` 只影响 Cookie 属性，不代表 WingmanKVM 自己提供 TLS
监听、证书管理或 TLS 终止。本服务仍按 `listen_address`/`port` 提供 HTTP；生产环境
需要在受信任的反向代理或隧道处终止 TLS，并把 HTTPS 源站正确转发到本服务。

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
