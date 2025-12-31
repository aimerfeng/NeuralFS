# NeuralFS Custom Protocol (nfs://)

This module provides the `nfs://` custom protocol for serving assets securely to the frontend.

## Overview

The custom protocol allows the frontend to load thumbnails, previews, and files directly without HTTP overhead, while maintaining security through session token validation.

## Backend Implementation

### Protocol Registration

The protocol is registered in `main.rs`:

```rust
use neural_fs::protocol::{register_custom_protocol, ProtocolState, get_session_token};
use neural_fs::asset::AssetServerConfig;

fn main() {
    // Create protocol state
    let asset_config = AssetServerConfig::default();
    let protocol_state = ProtocolState::new(
        neural_fs::asset::AssetServerState::new(asset_config)
    );

    // Register protocol and manage state
    let builder = tauri::Builder::default()
        .manage(protocol_state.clone());
    let builder = register_custom_protocol(builder, protocol_state);

    builder
        .invoke_handler(tauri::generate_handler![
            get_session_token,
            // ... other commands
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### Session Token Command

The `get_session_token` command returns:

```typescript
interface SessionTokenResponse {
  token: string;        // Session token for authentication
  protocol_url: string; // "nfs://"
  http_url: string;     // "http://127.0.0.1:19283" (fallback)
}
```

## Frontend Implementation

### 1. Session Token Handshake (Critical: Must Complete Before Any Asset Loading)

```typescript
// src/lib/assetService.ts
import { invoke } from '@tauri-apps/api/tauri';

interface SessionTokenResponse {
  token: string;
  protocol_url: string;
  http_url: string;
}

class AssetService {
  private token: string | null = null;
  private protocolUrl: string = 'nfs://';
  private httpUrl: string = 'http://127.0.0.1:19283';
  private initialized: boolean = false;
  private initPromise: Promise<void> | null = null;

  /**
   * Initialize the asset service by fetching the session token.
   * This MUST be called before any asset loading.
   */
  async initialize(): Promise<void> {
    if (this.initialized) return;
    
    // Prevent multiple simultaneous initializations
    if (this.initPromise) {
      return this.initPromise;
    }

    this.initPromise = this._doInitialize();
    return this.initPromise;
  }

  private async _doInitialize(): Promise<void> {
    try {
      const response = await invoke<SessionTokenResponse>('get_session_token');
      this.token = response.token;
      this.protocolUrl = response.protocol_url;
      this.httpUrl = response.http_url;
      this.initialized = true;
      console.log('Asset service initialized successfully');
    } catch (error) {
      console.error('Failed to initialize asset service:', error);
      throw error;
    }
  }

  /**
   * Ensure the service is initialized before use.
   */
  async ensureInitialized(): Promise<void> {
    if (!this.initialized) {
      await this.initialize();
    }
  }

  /**
   * Get the thumbnail URL for a file.
   * @param uuid File UUID
   * @param useProtocol Use nfs:// protocol (true) or HTTP fallback (false)
   */
  getThumbnailUrl(uuid: string, useProtocol: boolean = true): string {
    if (!this.token) {
      throw new Error('Asset service not initialized. Call initialize() first.');
    }
    
    if (useProtocol) {
      return `${this.protocolUrl}thumbnail/${uuid}?token=${this.token}`;
    }
    return `${this.httpUrl}/thumbnail/${uuid}?token=${this.token}`;
  }

  /**
   * Get the preview URL for a file.
   */
  getPreviewUrl(uuid: string, useProtocol: boolean = true): string {
    if (!this.token) {
      throw new Error('Asset service not initialized. Call initialize() first.');
    }
    
    if (useProtocol) {
      return `${this.protocolUrl}preview/${uuid}?token=${this.token}`;
    }
    return `${this.httpUrl}/preview/${uuid}?token=${this.token}`;
  }

  /**
   * Get the file URL for a file.
   */
  getFileUrl(uuid: string, useProtocol: boolean = true): string {
    if (!this.token) {
      throw new Error('Asset service not initialized. Call initialize() first.');
    }
    
    if (useProtocol) {
      return `${this.protocolUrl}file/${uuid}?token=${this.token}`;
    }
    return `${this.httpUrl}/file/${uuid}?token=${this.token}`;
  }

  /**
   * Get the session token (for custom requests).
   */
  getToken(): string | null {
    return this.token;
  }

  /**
   * Check if the service is initialized.
   */
  isInitialized(): boolean {
    return this.initialized;
  }
}

// Singleton instance
export const assetService = new AssetService();
```

### 2. App Initialization (SolidJS Example)

```typescript
// src/App.tsx
import { onMount, createSignal, Show } from 'solid-js';
import { assetService } from './lib/assetService';

function App() {
  const [isReady, setIsReady] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  onMount(async () => {
    try {
      // CRITICAL: Initialize asset service BEFORE rendering any images
      await assetService.initialize();
      setIsReady(true);
    } catch (err) {
      setError(`Failed to initialize: ${err}`);
    }
  });

  return (
    <Show when={isReady()} fallback={<LoadingScreen error={error()} />}>
      <MainApp />
    </Show>
  );
}

function LoadingScreen(props: { error: string | null }) {
  return (
    <div class="loading-screen">
      {props.error ? (
        <div class="error">{props.error}</div>
      ) : (
        <div class="spinner">Initializing...</div>
      )}
    </div>
  );
}
```

### 3. Fetch/Axios Interceptor (Optional)

If you need to make custom HTTP requests to the asset server:

```typescript
// src/lib/httpClient.ts
import { assetService } from './assetService';

// Fetch interceptor
export async function fetchWithToken(url: string, options: RequestInit = {}): Promise<Response> {
  await assetService.ensureInitialized();
  
  const token = assetService.getToken();
  if (!token) {
    throw new Error('No session token available');
  }

  // Add token to URL if it's an asset server request
  const assetServerUrl = 'http://127.0.0.1:19283';
  if (url.startsWith(assetServerUrl) || url.startsWith('nfs://')) {
    const separator = url.includes('?') ? '&' : '?';
    url = `${url}${separator}token=${token}`;
  }

  // Also add token to headers
  const headers = new Headers(options.headers);
  headers.set('X-Session-Token', token);

  return fetch(url, { ...options, headers });
}

// Axios interceptor example
import axios from 'axios';

export function setupAxiosInterceptor() {
  axios.interceptors.request.use(async (config) => {
    await assetService.ensureInitialized();
    
    const token = assetService.getToken();
    if (token) {
      // Add token to headers
      config.headers['X-Session-Token'] = token;
      
      // Add token to URL for asset server requests
      if (config.url?.includes('127.0.0.1:19283') || config.url?.startsWith('nfs://')) {
        const separator = config.url.includes('?') ? '&' : '?';
        config.url = `${config.url}${separator}token=${token}`;
      }
    }
    
    return config;
  });
}
```

### 4. Image Component Example

```typescript
// src/components/ThumbnailImage.tsx
import { createSignal, onMount, Show } from 'solid-js';
import { assetService } from '../lib/assetService';

interface ThumbnailImageProps {
  uuid: string;
  alt?: string;
  class?: string;
}

export function ThumbnailImage(props: ThumbnailImageProps) {
  const [src, setSrc] = createSignal<string | null>(null);
  const [error, setError] = createSignal(false);

  onMount(async () => {
    try {
      await assetService.ensureInitialized();
      setSrc(assetService.getThumbnailUrl(props.uuid));
    } catch (err) {
      console.error('Failed to get thumbnail URL:', err);
      setError(true);
    }
  });

  return (
    <Show when={!error()} fallback={<div class="thumbnail-error">Failed to load</div>}>
      <Show when={src()} fallback={<div class="thumbnail-loading">Loading...</div>}>
        <img
          src={src()!}
          alt={props.alt || 'Thumbnail'}
          class={props.class}
          onError={() => setError(true)}
        />
      </Show>
    </Show>
  );
}
```

## URL Formats

### nfs:// Protocol (Recommended)

- Thumbnail: `nfs://thumbnail/{uuid}?token={session_token}`
- Preview: `nfs://preview/{uuid}?token={session_token}`
- File: `nfs://file/{uuid}?token={session_token}`
- Health: `nfs://health/check?token={session_token}`

### HTTP Fallback

- Thumbnail: `http://127.0.0.1:19283/thumbnail/{uuid}?token={session_token}`
- Preview: `http://127.0.0.1:19283/preview/{uuid}?token={session_token}`
- File: `http://127.0.0.1:19283/file/{uuid}?token={session_token}`
- Health: `http://127.0.0.1:19283/health`

## Security

1. **Session Token**: Generated at app startup, required for all asset requests
2. **Token Validation**: Constant-time comparison to prevent timing attacks
3. **CSRF Protection**: Origin/Referer header validation
4. **Security Headers**: X-Content-Type-Options, X-Frame-Options, CSP, etc.
5. **Localhost Only**: Asset server binds only to 127.0.0.1

## Important Notes

1. **Initialize Before Loading**: The asset service MUST be initialized before any image/asset loading. Otherwise, all requests will return 403 Forbidden.

2. **Token Persistence**: The token is stored in memory only. It's regenerated on each app restart.

3. **Fallback Strategy**: If the nfs:// protocol doesn't work (e.g., in development), use the HTTP fallback URLs.

4. **Error Handling**: Always handle cases where the asset service fails to initialize.
