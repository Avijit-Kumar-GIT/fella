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
import Card from '../components/presentation/Card.svelte';
import Row from '../components/presentation/Row.svelte';
import Grid from '../components/presentation/Grid.svelte';
import Divider from '../components/presentation/Divider.svelte';
import Spacer from '../components/presentation/Spacer.svelte';
import Title from '../components/presentation/Title.svelte';
import Caption from '../components/presentation/Caption.svelte';
import Badge from '../components/presentation/Badge.svelte';
import Callout from '../components/presentation/Callout.svelte';
import Select from '../components/presentation/Select.svelte';
import SegmentedControl from '../components/presentation/SegmentedControl.svelte';
import Checkbox from '../components/presentation/Checkbox.svelte';
import Button from '../components/presentation/Button.svelte';
import Progress from '../components/presentation/Progress.svelte';
import { installPresentationFunctions } from './functions';

installPresentationFunctions();

export const presentationRenderers = { Document, Card, Section, Columns, Row, Grid, Divider, Spacer, Title, Text, Caption, Badge, Callout, Metric, Select, SegmentedControl, Checkbox, Button, Table, Chart, Progress, Tabs, ResultFilter: Filter, FollowUp: Action };

export const analyticalLibrary = createLibrary({
	root: 'Document',
	components: Object.entries(componentDefinitions).map(([name, definition]) => defineComponent({
		name, description: definition.description, props: definition.props,
		// The map is built from the common schema source; each registered renderer is statically
		// typed at its definition. Only this heterogeneous registry needs type erasure.
		component: presentationRenderers[name as keyof typeof presentationRenderers] as ComponentRenderer<any>
	}))
});

export { presentationSchema, presentationPrompt, presentationManifest, PRESENTATION_EXAMPLE, PRESENTATION_RULES, PRESENTATION_DESIGN_GUIDANCE } from './manifest';
