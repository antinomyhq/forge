// VSCode API singleton - can only be acquired once
let vscodeApi: any = null;

export function getVscodeApi() {
  if (!vscodeApi) {
    // @ts-ignore - acquireVsCodeApi is injected by VSCode
    vscodeApi = acquireVsCodeApi();
  }
  return vscodeApi;
}

export function postMessage(message: any) {
  getVscodeApi().postMessage(message);
}

export function onMessage(handler: (message: any) => void) {
  const listener = (event: MessageEvent) => {
    handler(event.data);
  };
  window.addEventListener('message', listener);
  return () => window.removeEventListener('message', listener);
}
