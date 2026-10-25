import { schema } from '@json-render/svelte/schema';
import { defineRegistry } from '@json-render/svelte';
import { componentDefinitions } from './schemas';
import Document from '../components/presentation/Document.svelte';
import Section from '../components/presentation/Section.svelte';
import Columns from '../components/presentation/Columns.svelte';
import Text from '../components/presentation/Text.svelte';
import Metric from '../components/presentation/Metric.svelte';
import Table from '../components/presentation/Table.svelte';
import Chart from '../components/presentation/Chart.svelte';
import Tabs from '../components/presentation/Tabs.svelte';
import TabPanel from '../components/presentation/TabPanel.svelte';
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

export const presentationComponents = {
	Document, Card, Section, Columns, Row, Grid, Divider, Spacer, Title, Text, Caption, Badge, Callout,
	Metric, Select, SegmentedControl, Checkbox, Button, Table, Chart, Progress, Tabs, TabPanel,
	ResultFilter: Filter, FollowUp: Action
};

const catalogComponents = Object.fromEntries(Object.entries(componentDefinitions).map(([name, definition]) => [name, {
	props: definition.props,
	slots: [...definition.slots],
	description: definition.description
}]));

/** The catalog is also the source for model output validation and schema generation. */
export const presentationCatalog = schema.createCatalog({ components: catalogComponents, actions: {} });
const registryResult = defineRegistry(presentationCatalog, { components: presentationComponents as never });
export const presentationRegistry = registryResult.registry;
