import { schema } from '@json-render/svelte/schema';
import { componentDefinitions } from './schemas';

// Schema generation must not import renderers or their compiled WASM asset.
const components = Object.fromEntries(Object.entries(componentDefinitions).map(([name, definition])=>[name,{props:definition.props,slots:[...definition.slots],description:definition.description}]));
export const presentationCatalog = schema.createCatalog({components,actions:{}});
