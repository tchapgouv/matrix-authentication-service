import { queryOptions, useSuspenseQuery } from "@tanstack/react-query";
import { Form } from "@vector-im/compound-web";
import { graphql } from "../../gql";
import { graphqlRequest } from "../../graphql";
import { mountWithProviders } from "../mount";
import "../shared.css";
import PasswordCreationDoubleInput from "./PasswordCreationDoubleInput";

const HTML_ID = "#password-double-input";

// :tchap: renamed from `PasswordChange` to avoid a name collision with the
// upstream query of the same name (frontend/src/routes/password.change.index.tsx)
const QUERY = graphql(/* GraphQL */ `
  query PasswordDoubleInput {
    viewer {
      __typename
      ... on Node {
        id
      }
    }

    siteConfig {
      ...PasswordCreationDoubleInput_siteConfig
    }
  }
`);

const query = queryOptions({
  queryKey: ["passwordChange"],
  queryFn: ({ signal }) => graphqlRequest({ query: QUERY, signal }),
});

type PasswordDoubleInputProps = { forceShowNewPasswordInvalid: boolean };

function PasswordDoubleInput({
  forceShowNewPasswordInvalid,
}: PasswordDoubleInputProps) {
  const {
    data: { siteConfig },
  } = useSuspenseQuery(query);
  return (
    // Form.Root is needed because Form.Field requires to be included into a Form
    // asChild allows to replace Form.Root component by the child, the <form> used is in the password.html
    <Form.Root asChild>
      <div>
        <PasswordCreationDoubleInput
          siteConfig={siteConfig}
          forceShowNewPasswordInvalid={forceShowNewPasswordInvalid}
        />
      </div>
    </Form.Root>
  );
}

// Allow mounting under either the new specific id or the legacy #view
mountWithProviders(HTML_ID, PasswordDoubleInput);
