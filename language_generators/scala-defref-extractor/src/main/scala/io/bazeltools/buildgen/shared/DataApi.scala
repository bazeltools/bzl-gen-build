package io.bazeltools.buildgen.shared

import scala.collection.immutable.SortedSet
import cats.kernel.Monoid
import io.circe.Encoder
import io.circe.generic.semiauto.deriveEncoder

case class ExtractedData(
    data_blocks: List[DataBlock],
    label_or_repo_path: String
)

object ExtractedData {
  implicit val extractedDataEncoder: Encoder[ExtractedData] =
    deriveEncoder[ExtractedData]
}

final case class Symbols(
    defs: SortedSet[Entity],
    refs: SortedSet[Entity],
    bzl_gen_build_commands: SortedSet[String]
) {
  def withEntityPath(epath: String): DataBlock =
    DataBlock(epath, defs, refs, bzl_gen_build_commands)

  def addDef(e: Entity) = copy(defs = defs + e)
  def addRef(e: Entity) = copy(refs = refs + e)
  def addBzlBuildGenCommand(e: String) =
    copy(bzl_gen_build_commands = bzl_gen_build_commands + e)
  def addBzlBuildGenCommands(e: Iterable[String]) =
    copy(bzl_gen_build_commands = bzl_gen_build_commands ++ e)
}

object Symbols {
  val empty: Symbols =
    Symbols(SortedSet.empty, SortedSet.empty, SortedSet.empty)

  implicit val SymbolsMonoid: Monoid[Symbols] =
    new Monoid[Symbols] {
      def empty = Symbols.empty
      def combine(left: Symbols, right: Symbols) =
        Symbols(
          left.defs | right.defs,
          left.refs | right.refs,
          left.bzl_gen_build_commands | right.bzl_gen_build_commands
        )
    }
}

final case class DataBlock(
    entity_path: String,
    defs: SortedSet[Entity],
    refs: SortedSet[Entity],
    bzl_gen_build_commands: SortedSet[String]
)

object DataBlock {
  implicit val dataBlockEncoder: Encoder[DataBlock] = deriveEncoder[DataBlock]
}
