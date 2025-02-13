package io.bazeltools.buildgen.shared

import cats.data.{Chain, NonEmptyList}
import cats.Order
import cats.parse.{Parser, Parser0}
import io.circe.{Decoder, Encoder}

import cats.syntax.all._

final case class Entity(parts: Vector[String]) {
  def init: Entity = Entity(parts.init)

  def prefixes: NonEmptyList[Entity] =
    // inits always returns an nonempty list
    NonEmptyList.fromListUnsafe(parts.inits.toList.map(Entity(_)))

  def resolve(fn: String => Entity.Resolved): Entity.Resolved =
    fn(parts.head) / Entity(parts.tail)

  def isSingleton: Boolean = parts.lengthCompare(1) == 0

  def startsWith(that: Entity): Boolean =
    (that.parts.length <= parts.length) &&
      parts.iterator.zip(that.parts.iterator).forall { case (a, b) => a == b }

  def asString: String = parts.mkString(".")

  def /(that: String): Entity = {
    require(that.nonEmpty)
    Entity(parts :+ that)
  }

  def /(that: Entity): Entity =
    Entity(parts ++ that.parts)

  override def toString(): String = asString
}

object Entity {

  // We assume that imports starting with special TLD (e.g. "com")
  // will never be a continuation of a previous wildcard import.
  //
  // This is to prevent a combinatorial explosion when we see code
  // such as:
  //
  //    import java.Math._
  //    <hundreds of imports starting with com>
  //
  // This would break if we ever see code which imports
  // `com.foo.com.bar.Qux` as:
  //
  //    import com.foo._
  //    import com.bar.Qux
  //
  // Note that we must avoid breaking imports like:
  //
  //    import com.foo.com.bar.Qux
  //    import com.acme.shadow.com.google.Dingus
  //
  // We could also handle other common TLDs such as "net" and "org"
  // the same way but "com" is the most common and one of the least
  // likely to occur as a "split import".
  //
  // This feature is disabled by default, and enabled in the driver
  // application using the environment variable BZL_GEN_SPECIAL_TLDS.
  private var specialTlds: Map[String, Entity.Resolved] =
    Map.empty

  def setSpecialTlds(names: List[String]): Unit = {
    specialTlds = names.iterator.map { name =>
      (name, Entity.Resolved.Known(Entity.simple(name)))
    }.toMap
  }

  def isSpecialTld(name: String): Boolean =
    specialTlds.contains(name)

  def getSpecialTld(name: String): Option[Entity.Resolved] =
    specialTlds.get(name)

  implicit val entityEncoder: Encoder[Entity] =
    Encoder.encodeString.contramap(_.asString)

  implicit val entityDecoder: Decoder[Entity] =
    Decoder.decodeString.map(dotted(_))

  val empty: Entity = Entity(Vector.empty)

  def simple(s: String): Entity = {
    require(s.nonEmpty)
    Entity(Vector(s))
  }

  def dotted(s: String): Entity =
    Entity(s.split("\\.", -1).toVector)

  implicit val catsOrderEntity: Order[Entity] =
    Order[Vector[String]].contramap[Entity](_.parts)

  implicit val entityOrdering: Ordering[Entity] =
    catsOrderEntity.toOrdering

  sealed abstract class Resolved {
    def /(that: String): Resolved

    def /(that: Entity): Resolved =
      that.parts.foldLeft(this)(_ / _)

    def |(that: Resolved): Resolved =
      Resolved.Many(this, that)

    // These are all the non-local entities
    def entities: Chain[Entity]
  }
  object Resolved {
    sealed abstract class Definite extends Resolved
    case class Imported(original: Resolved) extends Definite {
      def /(that: String) = Imported(original / that)

      def entities = original.entities
    }
    case class Local(scopePath: List[Long], entity: Entity) extends Definite {
      def /(that: String) = Local(scopePath, entity / that)
      def entities = Chain.empty
    }
    case class Known(name: Entity) extends Definite {
      def /(that: String) = Known(name / that)
      def entities = Chain.one(name)
    }
    case class Many(left: Resolved, right: Resolved) extends Resolved {
      def /(that: String) = Many(left / that, right / that)

      def entities = left.entities ++ right.entities
    }
  }

  object Parsers {
    def notThen[A](n: Parser0[Any], t: Parser[A]): Parser[A] =
      (!n).with1 *> t

    // extract all the items we can find by ignoreing
    // characters that start at an epsilon failure
    def repSkip[A](item: Parser[A]): Parser0[List[A]] = {
      val notItem = Parsers.notThen(item, Parser.anyChar).rep

      item.repSep0(notItem).surroundedBy(notItem.?)
    }
  }

  lazy val parseDirectives: Parser0[List[String]] = {
    // bzl_gen_build:dir:entity
    // Alternative here, for other name we've used
    val spaces0 = Parser.charIn(" \t").rep0
    val prefix = Parser.string("bzl_gen_build")
    val bzlBuildGen =
      (prefix.surroundedBy(spaces0) *> Parser.string(":")).surroundedBy(spaces0)

    val newLine = Parser.charIn("\r\n").void

    Parsers.repSkip(
      bzlBuildGen *> Parsers.notThen(newLine, Parser.anyChar).rep.string
    )
  }

  def findDirectives(str: String): Either[String, Chain[String]] = {
    parseDirectives
      .parseAll(str)
      .map(Chain.fromSeq(_))
      .leftMap(_.toString)
  }
}
