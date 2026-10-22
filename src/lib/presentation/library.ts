import { createLibrary, defineComponent, type ComponentRenderer } from '@openuidev/svelte-lang';
import { componentDefinitions } from './schemas';
import Document from '../components/presentation/Document.svelte';
import Section from '../components/presentation/Section.svelte';
import Columns from '../components/presentation/Columns.svelte';
import Text from '../components/presentation/Text.svelte';
import Metric from '../components/presentation/Metric.svelte';
import Table from '../components/presentation/Table.svelte';
import Chart from '../components/presentation/Chart.svelte';
import Tabs from '../components/presentation/Tabs.svelte';
import Filter from '../components/presentation/Filter.svelte';
import Action from '../components/presentation/Action.svelte';

const renderers = { Document, Section, Columns, Text, Metric, Table, Chart, Tabs, ResultFilter: Filter, FollowUp: Action };

export const analyticalLibrary = createLibrary({
	root: 'Document',
	components: Object.entries(componentDefinitions).map(([name, definition]) => defineComponent({
		name, description: definition.description, props: definition.props,
		// The map is built from the common schema source; each registered renderer is statically
		// typed at its definition. Only this heterogeneous registry needs type erasure.
		component: renderers[name as keyof typeof renderers] as ComponentRenderer<any>
	}))
});

export { presentationSchema, presentationPrompt, presentationManifest, PRESENTATION_EXAMPLE, PRESENTATION_RULES, PRESENTATION_DESIGN_GUIDANCE } from './manifest';
