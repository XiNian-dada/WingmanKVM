#!/usr/bin/env python3
"""Passkey button feedback and cancellation races; mock credentials only."""
import json
from pathlib import Path
from playwright.sync_api import sync_playwright
import ui_harness
from verify_p2_ui import State, api_route

STUB = r"""
window.__passkeyCalls = [];
PublicKeyCredential.isConditionalMediationAvailable = async () => window.__conditionalAvailable;
const credential = () => ({id:'fixture',type:'public-key',rawId:new Uint8Array([1,2,3]).buffer,
  response:{clientDataJSON:new Uint8Array([4]).buffer,authenticatorData:new Uint8Array([5]).buffer,
    signature:new Uint8Array([6]).buffer,userHandle:null},getClientExtensionResults:()=>({})});
navigator.credentials.get = async options => {
  window.__passkeyCalls.push(options.mediation || 'explicit');
  if(window.__passkeyMode === 'hold') return new Promise((resolve,reject) => {
    window.__resolvePasskey = () => resolve(credential());
    options.signal.addEventListener('abort', () => reject(new DOMException('Aborted','AbortError')), {once:true});
  });
  if(window.__passkeyMode === 'success') return credential();
  if(window.__passkeyMode === 'null') return null;
  if(window.__passkeyMode === 'security') throw new DOMException('Invalid relying party','SecurityError');
  if(window.__passkeyMode === 'cancel') throw new DOMException('User cancelled','NotAllowedError');
  throw new DOMException('WebAuthn is not supported on sites with TLS certificate errors.','NotAllowedError');
};
navigator.credentials.create = async () => { throw new DOMException('TLS certificate errors','NotAllowedError'); };
"""
OPTIONS = {"challenge_id":"fixture-challenge", "options":{"publicKey":{
    "challenge":"AQIDBA", "rpId":"localhost", "allowCredentials":[{"type":"public-key","id":"AQID"}],
    "timeout":120000}}}


def main():
    args=ui_harness.parse_args()
    summary=ui_harness.new_summary(args.repo)
    checks=summary['checks']
    check=lambda ok,name,evidence='':ui_harness.check(checks,ok,name,evidence)
    try:
        with ui_harness.static_server(args.repo) as port, sync_playwright() as p:
            browser=ui_harness.launch_browser(p,args.browser_channel)
            def open_page(mode='certificate', conditional=False, hold_start=False, authenticated=False):
                context=browser.new_context(viewport={"width":1000,"height":900})
                page=context.new_page()
                state=State(authenticated=authenticated,has_passkeys=True,mass_storage=False)
                page.add_init_script(f"window.__passkeyMode={json.dumps(mode)}; window.__conditionalAvailable={json.dumps(conditional)};"+STUB)
                page.route('**/api/**',lambda route:api_route(route,state))
                page.route('**/video_feed*',lambda route:route.fulfill(status=200,content_type='image/gif',body=ui_harness.GIF))
                page.route('**/api/tls',lambda route:route.fulfill(status=200,content_type='application/json',body='{"mode":"disabled","enabled":false}'))
                starts, held, finishes=[],[],[]
                def start(route):
                    starts.append(route)
                    if hold_start and len(starts)==1: held.append(route)
                    else: route.fulfill(status=200,content_type='application/json',body=json.dumps(OPTIONS))
                def finish(route):
                    finishes.append(json.loads(route.request.post_data))
                    state.authenticated=True
                    route.fulfill(status=200,content_type='application/json',body='{}')
                page.route('**/api/passkey/login/start',start)
                page.route('**/api/passkey/login/finish',finish)
                page.route('**/api/passkey/register/start',lambda route:route.fulfill(status=200,content_type='application/json',body=json.dumps({
                    "challenge_id":"register-fixture","options":{"publicKey":{"challenge":"AQID","user":{"id":"AQID"}}}})))
                page.goto(f'http://localhost:{port}/',wait_until='load')
                page.wait_for_selector('#app:not(.hidden)' if authenticated else '#passkey-login-btn:visible')
                return context,page,starts,held,finishes

            for mode,expected in [('certificate','证书未受信任'),('cancel','验证未完成'),('security','浏览器拒绝'),('null','验证未完成')]:
                context,page,starts,_,finishes=open_page(mode)
                page.click('#passkey-login-btn')
                page.wait_for_function("document.querySelector('#passkey-login-error').textContent.length > 0")
                message=page.locator('#passkey-login-error').inner_text()
                check(expected in message,mode+'_failure_is_visible',message)
                check(page.locator('#passkey-login-btn').is_enabled() and not finishes,mode+'_retry_enabled_no_false_login')
                if mode=='certificate' and args.output_dir:
                    Path(args.output_dir).mkdir(parents=True,exist_ok=True)
                    page.screenshot(path=str(Path(args.output_dir)/'passkey-certificate-feedback.png'))
                    page.set_viewport_size({'width':390,'height':844})
                    check(page.locator('#login-form').evaluate('e => e.scrollWidth <= e.clientWidth+1'),'certificate_help_fits_mobile')
                    page.screenshot(path=str(Path(args.output_dir)/'passkey-certificate-mobile.png'))
                context.close()

            context,page,starts,held,finishes=open_page('hold',conditional=True,hold_start=True)
            for _ in range(100):
                if held: break
                page.wait_for_timeout(20)
            check(len(held)==1,'autofill_start_is_pending')
            page.click('#passkey-login-btn')
            page.wait_for_function("window.__passkeyCalls.length===1")
            held[0].fulfill(status=200,content_type='application/json',body=json.dumps(OPTIONS))
            page.wait_for_timeout(100)
            check(page.evaluate('window.__passkeyCalls')==['explicit'],'late_autofill_does_not_steal_explicit_request')
            check(page.locator('#passkey-login-btn').get_attribute('aria-busy')=='true'
                  and '请完成' in page.locator('#passkey-login-btn').inner_text(),'button_explains_pending_verification')
            page.locator('#passkey-login-btn').evaluate("e => e.dispatchEvent(new Event('click'))")
            check(len(starts)==2,'duplicate_explicit_click_is_ignored')
            page.evaluate('window.__resolvePasskey()')
            page.wait_for_selector('#app:not(.hidden)')
            check(len(finishes)==1 and finishes[0]['credential']['response']['signature']=='Bg','successful_explicit_login_still_finishes')
            context.close()

            context,page,starts,_,_=open_page('hold',hold_start=True)
            page.click('#passkey-login-btn')
            check('正在打开' in page.locator('#passkey-login-btn').inner_text(),'button_explains_pending_network_request')
            starts[0].abort()
            page.wait_for_function("!document.querySelector('#passkey-login-btn').disabled")
            context.close()

            context,page,_,_,_=open_page('hold')
            page.clock.install()
            page.click('#passkey-login-btn')
            page.wait_for_function('window.__passkeyCalls.length===1')
            page.clock.fast_forward(61000)
            page.wait_for_function("document.querySelector('#passkey-login-error').textContent.includes('超时')")
            check(page.locator('#passkey-login-btn').is_enabled(),'timeout_restores_button_and_explains_failure')
            context.close()

            context,page,_,_,_=open_page(authenticated=True)
            page.on('dialog',lambda dialog:dialog.accept('Fixture device'))
            page.click('#settings-toggle')
            page.locator('#passkey-add-btn').click()
            page.wait_for_function("document.querySelector('#passkey-inspector-hint').textContent.includes('证书未受信任')")
            check(page.locator('#passkey-inspector-hint').is_visible(),'registration_certificate_error_is_persistent')
            context.close()
            browser.close()
            summary['status']='PASS'
    except Exception as error:
        summary['error']=f'{type(error).__name__}: {error}'
    return ui_harness.finish(summary,args.output_dir,'passkey_ui_report.json')

if __name__=='__main__':
    raise SystemExit(main())
